# Security

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub: open the repository's
**Security** tab and choose **Report a vulnerability**. Do not open a public
issue for a security problem.

Include the NeuroSkill™ version, your OS, and the steps to reproduce.

## Remote access: iroh clients and TOTP

Remote devices (for example a phone) reach the local daemon over an iroh
tunnel. A device is allowed in only after it registers its iroh endpoint ID
with a one-time code from a TOTP credential created on this machine
(Settings → Clients, or `iroh totp create <name>`; see [SKILL.md](SKILL.md)).

### TOTP parameters

Codes follow RFC 6238: HMAC-SHA1, 6 digits, a 30-second step, and a skew of
one step, so a code is valid from the step before to the step after the
current one (about ±30 s of clock drift).

### Each code is accepted once

RFC 6238 §5.2 requires a verifier to accept a code only once. Each TOTP
credential records the time step of the last code it accepted
(`last_used_step`), and registration rejects any code from that step or an
earlier one with `otp already used; wait for the next code`. Without this, the
skew window would let a code that was observed or intercepted be replayed for
roughly a minute.

The limit applies per credential, not per device. Pairing a second device, or
re-registering one, with the same credential needs the next code, which can
take up to 30 seconds to appear. A failed attempt does not use up a code.

Credentials created before this check existed have no recorded step; their
first accepted code starts the record.

### Failed codes lock pairing

Peers that are not yet registered can reach the registration endpoint, so
codes could otherwise be guessed. After 5 wrong codes in a row, registration
is refused for 60 seconds, even with a correct code. Each further wrong code
doubles the wait, up to one hour. A successful registration resets the count.

The count is shared by everyone trying to register, because a caller chooses
its own endpoint ID and could switch to a new one to dodge a per-caller limit.
Someone guessing codes can therefore delay a legitimate pairing. At the
one-hour cap they get about 24 guesses a day, each with a 3-in-a-million chance
(the current code and the two skew neighbours). The count is kept in memory and resets when the daemon restarts.

### Scopes and revocation

- A registered client gets `read` scope unless `full` is requested. `full`
  gives remote control over the whole local API; grant it only to devices you
  own.
- Revoking a client removes its access. Revoking a TOTP credential also
  revokes every client registered with it.

### Where secrets live

TOTP secrets and the client list are stored in `iroh_auth.json` in the
NeuroSkill data directory. The secrets are not encrypted, so anyone who can
read that file can generate valid codes. Treat it like a password file.

On macOS and Linux the file is written readable and writable by its owner only
(mode `0600`), and a file left readable by others by an older version is
tightened when the daemon opens it. On Windows it relies on the permissions of
the per-user data directory. Each save writes a temporary file and renames it
into place, so a crash cannot leave a half-written file.
