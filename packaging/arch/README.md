# Utre on Arch Linux

The main install guide is in the [root README](../../README.md#install). This page has the same steps.

## Prebuilt, from our own pacman repository (recommended)

The repository `utre` lives on the `arch-repo` GitHub release and is refreshed by
`scripts/pacman-repo-publish.sh` on every release. Packages and the database are signed. Trust the key
first (check that the fingerprint is `8D9A9E543BB6D88237D174A78BE15C9843ADD341`):

```
curl -LO https://raw.githubusercontent.com/Belkata/utre/main/packaging/arch/utre-repo.asc
sudo pacman-key --add utre-repo.asc
sudo pacman-key --lsign-key 8D9A9E543BB6D88237D174A78BE15C9843ADD341
```

Then add this to `/etc/pacman.conf` (default signature level):

```
[utre]
Server = https://github.com/Belkata/utre/releases/download/arch-repo
```

Install once, then start the daemon:

```
sudo pacman -Syu utre-bin
systemctl --user daemon-reload && systemctl --user enable --now utre.service
```

After that `pacman -Syu` or `yay -Syu` updates the app, the daemon and the `utre` CLI. Restart the
daemon after an update: `systemctl --user restart utre.service`.

## From source

`packaging/arch/PKGBUILD` (`utre-git`) builds the main branch: `cd packaging/arch && makepkg -si`.
It conflicts with `utre-bin`.
