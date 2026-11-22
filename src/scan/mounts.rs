//! Which devices the default scan does not enter: pseudo and network filesystems.
//! Read from `/proc/self/mountinfo`; unreadable means nothing is skipped.

use std::collections::HashSet;

use rustix::fs::makedev;

const MOUNTINFO: &str = "/proc/self/mountinfo";

/// Filesystems that hold no files of their own (kernel views) or that live elsewhere (network).
fn is_pseudo(fstype: &str) -> bool {
    const KERNEL: &[&str] = &[
        "proc",
        "sysfs",
        "devtmpfs",
        "devpts",
        "cgroup",
        "cgroup2",
        "securityfs",
        "debugfs",
        "tracefs",
        "bpf",
        "pstore",
        "configfs",
        "fusectl",
        "mqueue",
        "hugetlbfs",
        "autofs",
        "binfmt_misc",
        "efivarfs",
        "selinuxfs",
        "rpc_pipefs",
        "nsfs",
    ];
    const NETWORK: &[&str] = &[
        "nfs",
        "nfs4",
        "cifs",
        "smb3",
        "smbfs",
        "9p",
        "afs",
        "ceph",
        "glusterfs",
        "lustre",
        "ncpfs",
    ];
    KERNEL.contains(&fstype) || NETWORK.contains(&fstype) || fstype.starts_with("fuse.")
}

/// Devices whose every mount is pseudo or network, as `st_dev` values.
pub fn skipped_devs(mountinfo: &str) -> HashSet<u64> {
    let (mut pseudo, mut real) = (HashSet::new(), HashSet::new());
    for line in mountinfo.lines() {
        let Some((left, right)) = line.split_once(" - ") else {
            continue;
        };
        let Some((major, minor)) = left
            .split(' ')
            .nth(2)
            .and_then(|f| f.split_once(':'))
            .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
        else {
            continue;
        };
        let dev = makedev(major, minor);
        let target = if is_pseudo(right.split(' ').next().unwrap_or("")) {
            &mut pseudo
        } else {
            &mut real
        };
        target.insert(dev);
    }
    pseudo.difference(&real).copied().collect()
}

/// The skip set for this machine.
pub fn system_skipped_devs() -> HashSet<u64> {
    std::fs::read_to_string(MOUNTINFO).map_or_else(|_| HashSet::new(), |t| skipped_devs(&t))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &str = "\
24 1 0:21 / / rw - btrfs /dev/nvme0n1p2 rw
25 24 0:22 / /proc rw - proc proc rw
26 24 0:6 / /dev rw - devtmpfs dev rw
27 24 0:30 / /tmp rw - tmpfs tmpfs rw
28 24 0:40 / /mnt/nas rw - nfs4 srv:/x rw
29 24 0:41 / /run/user/1000/doc rw - fuse.portal portal rw
30 24 0:50 /a /x rw - btrfs /dev/nvme0n1p2 rw
31 24 0:50 /a /y rw - proc proc rw
32 24 0:60 /a /home rw shared:1 - ext4 /dev/sda1 rw";

    #[test]
    fn skips_kernel_network_and_fuse_but_keeps_real_and_tmpfs() {
        let s = skipped_devs(INFO);
        for (a, b) in [(0, 22), (0, 6), (0, 40), (0, 41)] {
            assert!(s.contains(&makedev(a, b)), "{a}:{b}");
        }
        for (a, b) in [(0, 21), (0, 30), (0, 50), (0, 60)] {
            assert!(!s.contains(&makedev(a, b)), "{a}:{b}");
        }
    }

    #[test]
    fn garbage_skips_nothing() {
        assert!(skipped_devs("nonsense\n\n - \n").is_empty());
    }
}
