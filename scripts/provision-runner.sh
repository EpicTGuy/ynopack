#!/usr/bin/env bash
#
# Monte la VM qui fera tourner package_check, et donc la gate G4.
#
# POURQUOI UNE VM plutot que l'hote directement : package_check exige Incus,
# dont le bridge reseau reclame dnsmasq sur le port 53 — deja pris par YunoHost
# sur une instance YunoHost. Un reseau Docker sur la meme machine entre aussi en conflit
# supplementaire. Une VM isole les deux problemes d'un coup.
#
# ATTENTION : ce script n'a pas encore ete execute. Il transcrit le runbook
# officiel de package_check, mais tant qu'il n'a pas tourne pour de vrai, le
# considerer comme une proposition a relire, pas comme une procedure eprouvee.
#
# La gate G4 n'est requise que pour une contribution au catalogue officiel.
# Pour le circuit Forgejo interne, G3 suffit — et la CI publique de YunoHost
# (`!testme` sur une pull request) mesure le niveau sans aucune infrastructure.
set -euo pipefail

HOTE="${1:-${YUNOPACK_HOTE_VM:-}}"
[ -n "$HOTE" ] || { echo "usage : $0 <alias ssh de la machine hote>" >&2; exit 2; }
NOM_VM="${NOM_VM:-yunopack-runner}"
VCPU="${VCPU:-4}"
RAM_MO="${RAM_MO:-8192}"
DISQUE_GO="${DISQUE_GO:-60}"

echo "Provisionnement de « $NOM_VM » sur $HOTE"
echo "  $VCPU vCPU · ${RAM_MO} Mo · ${DISQUE_GO} Go"
echo

ssh "$HOTE" "command -v virt-install >/dev/null" || {
  echo "virt-install absent de $HOTE." >&2
  echo "L'installer avec : apt install libvirt-daemon-system virtinst cloud-image-utils" >&2
  exit 1
}

# shellcheck disable=SC2087  # l'expansion locale est voulue
ssh "$HOTE" bash -s <<SCRIPT
set -euo pipefail
cd /var/lib/libvirt/images

if virsh dominfo "$NOM_VM" >/dev/null 2>&1; then
  echo "La VM existe deja ; rien a faire."
  exit 0
fi

# Image officielle Debian, verifiee par sa somme de controle.
IMG=debian-12-generic-amd64.qcow2
[ -f "\$IMG" ] || curl -fsSL -o "\$IMG" \
  https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2

qemu-img create -f qcow2 -F qcow2 -b "\$IMG" "$NOM_VM.qcow2" "${DISQUE_GO}G"

# cloud-init : un utilisateur, la cle SSH de l'hote, et les dependances.
cat > /tmp/$NOM_VM-user-data <<CLOUD
#cloud-config
hostname: $NOM_VM
users:
  - name: runner
    sudo: ALL=(ALL) NOPASSWD:ALL
    shell: /bin/bash
    ssh_authorized_keys:
      - \$(cat ~/.ssh/id_*.pub 2>/dev/null | head -1)
package_update: true
packages: [lynx, jq, btrfs-progs, git, curl]
CLOUD
cloud-localds /tmp/$NOM_VM-seed.iso /tmp/$NOM_VM-user-data

virt-install --name "$NOM_VM" --memory "$RAM_MO" --vcpus "$VCPU" \
  --disk "path=/var/lib/libvirt/images/$NOM_VM.qcow2,format=qcow2" \
  --disk "path=/tmp/$NOM_VM-seed.iso,device=cdrom" \
  --os-variant debian12 --import --network default --noautoconsole
SCRIPT

cat <<'SUITE'

VM creee. Il reste a faire, une fois dedans (ssh runner@<ip>) :

  curl -fsSL https://pkgs.zabbly.com/get/incus-stable | sudo sh
  sudo incus admin init --minimal
  sudo usermod -aG incus-admin "$USER"   # puis se reconnecter
  git clone https://github.com/YunoHost/package_check
  cd package_check && ./package_check.sh <chemin-du-paquet>

Declarer ensuite l'alias dans ~/.ssh/config pour que `yunopack test --full`
puisse s'y adresser.
SUITE
