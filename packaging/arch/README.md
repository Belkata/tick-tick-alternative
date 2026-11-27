# Utre on Arch Linux

## Prebuilt, from our own pacman repository (recommended)

The repository `utre` lives on the `arch-repo` GitHub release and is refreshed by
`scripts/pacman-repo-publish.sh` on every release. Add this to `/etc/pacman.conf`:

```
[utre]
SigLevel = Optional TrustAll
Server = https://github.com/Belkata/utre/releases/download/arch-repo
```

Install once, then start the daemon:

```
sudo pacman -Sy utre-bin
systemctl --user daemon-reload && systemctl --user enable --now utre.service
```

After that `pacman -Syu` or `yay -Syu` updates the app, the daemon and the `utre` CLI. Restart the
daemon after an update: `systemctl --user restart utre.service`.

### Signed variant (once a signing key exists)

Packages and databases are signed when the publisher sets `UTRE_REPO_GPGKEY`. Then import the key
(a key file will be kept in this repo) and use the default signature level:

```
sudo pacman-key --add utre-repo.gpg
sudo pacman-key --lsign-key <KEYID>
```

```
[utre]
Server = https://github.com/Belkata/utre/releases/download/arch-repo
```

## From source

`packaging/arch/PKGBUILD` (`utre-git`) builds the main branch: `cd packaging/arch && makepkg -si`.
It conflicts with `utre-bin`.
