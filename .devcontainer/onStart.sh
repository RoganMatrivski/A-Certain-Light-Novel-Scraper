#!/bin/sh

# Add JSON: "postStartCommand": "chmod +x ./.devcontainer/onStart.sh; containerWorkspaceFolder=${containerWorkspaceFolder} ./.devcontainer/onStart.sh",

mount_folder="/mnt/docker-mnt";
sudo chown $USER:$USER $mount_folder;
link_folder="target";

for folder in $link_folder
do
    mkdir -p "$mount_folder/$folder"
    rm -rf ${containerWorkspaceFolder}/$folder
    ln -s $mount_folder/$folder ${containerWorkspaceFolder}/$folder
done

curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
cargo binstall -y --disable-telemetry worker-build && cargo binstall -y --disable-telemetry wasm-bindgen-cli
npm install -g wrangler
