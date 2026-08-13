# Eh's Releaser
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
![Issues](https://img.shields.io/github/issues/ehshit/releaser?style=flat&color=orange)
![GitHub Pull Requests](https://img.shields.io/github/issues-pr/ehshit/releaser)

Repo used for releasing multi-projects all in one for your needs all by being automated, so you really don't have to

it uses its own `.toml` file to know what to do, linked via a `meta.toml` file 

# How to set it up
When forking the repo, here's what you need to do configure this for yourself or your organization for each file

### `meta.toml`
| Value | Required | Description |
|-------------|:---:|---------------|
| `bot_name` | no | The bot's name, used in its footer |
| `approve_phrase` | no | Word to trigger a release |
| `categories` | **yes** | List of relative paths to category `.toml` files that describe your projects |

### Category `.toml`
| Value | Required | Description |
|-------------|:---:|---------------|
| `project` | no | List of projects to manage |

#### `[[project]]`
| Value | Required | Description |
|-------------|:---:|---------------|
| `id` | **yes** | Project ID that's unique |
| `repo` | **yes** | Repository to control, must match `name/repo` |
| `version` | **yes** | The version, written into `version_files` |
| `version_files` | no | List of files to bump the version in |
| `branch` | no | Target branch to push to, default on `main` |
| `changelog_file` | no | Path on the target where any changelog lives |
| `publish_workflow` | no | Workflow file to dispatch |
| `extension_name` | no | Passed as an input for some workflows (Zed) |
| `github_release` | no | If `true`, it does a GitHub release |
| `release_assets` | no | Uploads assets to the release |
| `pr` | no | If `true`, opens a PR in the target repo instead of just committing |
| `pr_template` | no | Template file path in the meta repo used for the PR body |
| `komac` | no | Uses Komac for pushing stuff to the winget repo |

##### `version_files`
| Value | Required | Description |
|-------------|:---:|---------------|
| `path` | **yes** | Path to the file to bump |
| `field` | no | Field to replace |
| `kind` | no | `json`, `toml`, `xml` or `text` |

##### `[komac]`
| Value | Required | Description |
|-------------|:---:|---------------|
| `manifest_repo` | **yes** | The winget manifest repo (the bot's fork) |
| `package` | **yes** | The `Package.Identifier` |
| `url_template` | no | The URL template to download, allows `{version}` |
| `url` | no | The URL to download, allows `{version}` |

## Examples
For an example of how it works you can look at the [examples](examples/) folder

## Bot 
The repo works best if you do a bot app for your org or your account instead of an automated bot account, which you can create [here](https://github.com/settings/apps), tho you can use a GitHub account bot by adding a `GH_PAT` secret to your repo settings, though it might be subject to TOS and other stuff.

## If Using a GitHub APP
When creating the bot, you must give the exact permissions:

- Contents to Read And Write
- Pull Requests to Read and Write
- Issues to Read and Write
- Actions to Read and Write

If the bot is to an organization, make sure you set Members to Read as well!

## If Using a GitHub PAT
If you are using a GitHub PAT, you must give the exact permissions:

- `repo`

If the bot is to an organization, make sure you tick `read:org` as well!

## Secrets and Variables
After you done that, and the bot is invited, you must add this to your Secrets and Variables (to the repo ones)

### Secrets
- `GH_PAT` only if a GitHub PAT is configured with your token (`HU_GH_TOKEN` for Zed or WinGet via Komac automation)
- `GH_APP_ID` GitHub App ID when creating your app
- `GH_APP_INSTALLATION_ID` The ID of the installation after it's installed to your account or organization
- `GH_APP_PRIVATE_KEY` The contents of the .pem key that you generated
- `OVSX_PAT` your [Open VSX token](https://open-vsx.org/user-settings/tokens)
- `JB_TOKEN` your [JetBrains Token](https://plugins.jetbrains.com/author/me/tokens)

### Variables
- `ALLOWED_USER` JSON array of users that can approve the bot to operate (using `<@botname> yes`), (Example is `["name1", "name2"]`)
- `APPROVE_PHRASE` and `TOML_APPROVE_PHRASE` being the exact wording used to approve the request and run the workflows to check those
- `ORG_NAME` The name of your organization by its url (Example `ehshit`), only if being on an organization
- `KOMAC_FORK` repo to your WinGet Manifests repo (if it publishes stuff to winget)
- `ZEDEX_FORK` repo to your Zed extensions repo (if it publishes stuff to Zed)