# Upgrading to newer version of polkadot

## Inspect the changes to the template

To do this you can use github compare release feature, where you can see the diff between two releases. For example the diff between the 0.9.24 and 0.9.27 upgrade is:
- https://github.com/substrate-developer-hub/substrate-parachain-template/compare/polkadot-v0.9.24...polkadot-v0.9.27

You can chose to port the changes manually or generate a patch file to apply.

## Generating a patch file

If you want to create a diff that you can apply, run the following after you checkout the [substrate-parachain-template](https://github.com/substrate-developer-hub/substrate-parachain-template) codebase:
```sh
# Shows all the changes between the two releases
git diff polkadot-v0.9.24 polkadot-v0.9.27
# Shows the names of the files changed between the two releases
git diff polkadot-v0.9.24 polkadot-v0.9.27  --name-only
# Show the changes of some paths between two releases
git diff polkadot-v0.9.24 polkadot-v0.9.27  -- path_1 path_2
# For example to show only the diff under the node folder and node/ pallets/template/Cargo.toml run:
git diff polkadot-v0.9.24 polkadot-v0.9.27  -- node/ pallets/template/Cargo.toml
# Stores all changes in a patch file
git diff polkadot-v0.9.24 polkadot-v0.9.27  > v0.9.24_to_v0.9.27_upgrade.diff
```
Generate a diff that you prefer including the files you want to apply. To apply it go to the avn-node-parachain-repo and run the following:
```sh
git apply <path_to_diff>/v0.9.24_to_v0.9.27_upgrade.diff --reject --ignore-whitespace
```
This will apply the changes that have no conflicts and create .rej files for the ones that could not be applied automatically.
Then inspect the .rej files and rectify case by case.

Once completed commit the changes and ensure the project builds.