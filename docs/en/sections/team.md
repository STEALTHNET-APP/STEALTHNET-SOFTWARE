[All sections](../README.md) · [Русский](../../ru/sections/team.md)

# First owner, profile and team

## First sign-in

On a clean server, the installer asks for the owner's username and password. Leave the password blank to generate a random one. After preparing the database, the installer creates an `owner` account through `sn-admin`. The administrative panel has no public registration endpoint.

Open the panel domain and sign in with those credentials. They are also saved in `/root/stealthnet-access.txt` with mode `600`. Move them to a password manager and delete that file. Updates neither recreate the owner nor change their password.

To recover a forgotten password, connect to the server over SSH as root and run:

```bash
stealthnet admin-password
```

Enter an existing username and the new password. The command preserves the account's role, closes its sessions and revokes its API tokens. Its 2FA settings remain enabled.

## Personal profile

Click your account at the bottom of the sidebar → **Admin profile** (`#/admin-profile`). Update your username and contact email by confirming your current password, change your password, enable TOTP, manage passkeys or close other sessions. Email is a contact address; sign-in uses your username.

## Support teammates

An owner opens their account menu → **Team and access** (`#/team`) → **Add teammate**. Choose a separate username, a 12–128 character password and a role. Give the teammate the panel address and sign-in details. They use the normal sign-in page, then change their password and enable 2FA in their own profile.

| Role | Access |
|---|---|
| Owner | Manages the service, customer cabinet and teammates |
| Administrator | Manages the service; cannot manage teammates or owner-only cabinet settings |
| Support | Views customers, payments, plans and network; manages notes/tags, replies to tickets, revokes connections and removes devices. Cannot change plans, purchases, limits or infrastructure |
| Read only | Reads panel data without changing the service; can manage personal security |

Manage a teammate to change their role, disable/enable access, replace their password or revoke their sessions. Role changes, disabling, password resets and explicit revocation close their sessions and revoke their API tokens. Reissue any tokens used by integrations. An owner cannot disable their own account or remove their own owner role.

Staff changes are recorded in the audit log without passwords. API authorization also applies to direct requests outside the UI. Customer accounts and staff accounts are separate entities.

[Installation](../installation.md) · [Security](../../../SECURITY.md) · [Support](support.md)
