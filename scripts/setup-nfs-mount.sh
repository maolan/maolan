#!/usr/bin/env bash
set -euo pipefail

# setup-nfs-mount.sh — Mount the host's ~/repos NFS share on the Ubuntu VM.
#
# This is intended to be run once on the VM. It installs nfs-common,
# adds the mount to /etc/fstab, and mounts it immediately.

NFS_SERVER="172.16.0.254"
NFS_EXPORT="/home/meka/repos"
MOUNT_POINT="$HOME/repos"

echo "Setting up NFS mount for build source..."

if ! command -v mount.nfs4 &>/dev/null; then
    echo "Installing nfs-common..."
    sudo apt-get update
    sudo apt-get install -y nfs-common
fi

mkdir -p "$MOUNT_POINT"

# Check if already mounted
if mountpoint -q "$MOUNT_POINT"; then
    echo "$MOUNT_POINT is already mounted."
    exit 0
fi

# Add to fstab if not already present
FSTAB_ENTRY="$NFS_SERVER:$NFS_EXPORT $MOUNT_POINT nfs defaults,_netdev 0 0"
if ! grep -qF "$FSTAB_ENTRY" /etc/fstab; then
    echo "$FSTAB_ENTRY" | sudo tee -a /etc/fstab
    echo "Added fstab entry."
else
    echo "fstab entry already exists."
fi

# Mount now
sudo mount "$MOUNT_POINT"
echo "NFS mount active at $MOUNT_POINT"
echo "Source directory: $MOUNT_POINT/maolan/daw"
