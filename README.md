# imshare

A lightweight Rust CLI tool that generates signed, expiring share links for an Immich photo library proxied via [immich-public-proxy](https://github.com/alangrainger/immich-public-proxy).

## Features

- Generate JWT-signed share links with configurable expiration
- Revoke links without deleting them from history
- Extend existing links (issues new token)
- SQLite-backed persistence
- Verification middleware with fail-closed security
- NixOS-native deployment

## Architecture

```
User → pub.nith.sh (Cloudflare Tunnel)
         ↓
    imshare-verify :3001 (validates JWT, checks revocation)
         ↓
    immich-public-proxy :3000
         ↓
    Immich
```

## Installation (NixOS)

### 1. Build with Flakes

```bash
# Enter development shell
nix develop

# Build binaries
cargo build --release

# Or build with Nix
nix build
```

### 2. Install Binaries

```bash
# Install to system (adjust paths as needed)
sudo cp target/release/imshare /usr/local/bin/
sudo cp target/release/imshare-verify /usr/local/bin/

# Or use the Nix-built binaries
sudo cp result/bin/imshare* /usr/local/bin/
```

### 3. Configure Secret

```bash
# Generate a strong secret
openssl rand -base64 32

# Create environment file
sudo mkdir -p /etc/imshare
sudo tee /etc/imshare/env << EOF
IMSHARE_SECRET=<your-generated-secret>
EOF
sudo chmod 600 /etc/imshare/env
```

### 4. Configure imshare

```bash
# Create config directory
mkdir -p ~/.config/imshare

# Copy example config
cp config.toml.example ~/.config/imshare/config.toml

# Edit to match your setup
nano ~/.config/imshare/config.toml
```

### 5. Enable systemd Service

Add to your `/etc/nixos/configuration.nix`:

```nix
{ config, pkgs, ... }:

{
  # Import the imshare flake module (adjust path to your clone)
  imports = [
    /path/to/imshare/flake.nix#nixosModules.default
  ];

  services.imshare-verify = {
    enable = true;
    environmentFile = "/etc/imshare/env";
  };

  # Or manually configure the service:
  systemd.services.imshare-verify = {
    enable = true;
    description = "imshare verification proxy";
    wantedBy = [ "multi-user.target" ];
    after = [ "network.target" ];

    serviceConfig = {
      ExecStart = "/usr/local/bin/imshare-verify";
      Restart = "always";
      RestartSec = "10s";
      EnvironmentFile = "/etc/imshare/env";
      User = "imshare";
      Group = "imshare";

      # Hardening
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = true;
      ReadWritePaths = [ "/var/lib/imshare" ];
    };
  };

  users.users.imshare = {
    isSystemUser = true;
    group = "imshare";
    home = "/var/lib/imshare";
    createHome = true;
  };

  users.groups.imshare = {};
}
```

Rebuild and switch:

```bash
sudo nixos-rebuild switch
```

### 6. Configure Cloudflare Tunnel

Update your Cloudflare Tunnel configuration to point to `localhost:3001` instead of `localhost:3000`:

```yaml
# cloudflared config.yml
ingress:
  - hostname: pub.nith.sh
    service: http://localhost:3001
  - service: http_status:404
```

Restart cloudflared:

```bash
sudo systemctl restart cloudflared
```

## Usage

### Generate a Link

```bash
# From Immich share URL
export IMSHARE_SECRET="your-secret"
imshare generate https://photos.sumoftheir.parts/share/abc-123-def

# From raw UUID
imshare generate abc-123-def

# With custom TTL and label
imshare generate abc-123-def --ttl 7d --label "Big Sur Trip"

# Unlimited expiry
imshare generate abc-123-def --ttl unlimited
```

TTL formats: `7d`, `24h`, `1w`, `30d`, `1m`, `1y`, `unlimited`

### List All Links

```bash
imshare list
```

Output:
```
ID    Label                Album ID                  Expires                Status
-------------------------------------------------------------------------------------
3     Big Sur Trip         abc-123-def               2026-04-09 15:30 UTC   active
2     -                    xyz-789-ghi               unlimited              active
1     Family Photos        def-456-jkl               2026-03-15 10:00 UTC   expired
```

### Revoke a Link

```bash
imshare revoke 3
```

Revoked links remain in the database for audit purposes but are rejected by `imshare-verify`.

### Extend a Link

```bash
imshare extend 3 14d
```

**⚠️ IMPORTANT CAVEAT**: The `extend` command issues a **new token with a new JTI**, which means:
- The old URL is **immediately invalidated**
- Any previously shared links will **stop working**
- You must **share the new URL** with recipients

This is by design for security - there's no way to modify a JWT without reissuing it.

## Verification Middleware

`imshare-verify` validates every request before proxying to immich-public-proxy:

### Static Resources (`/share/static/*`)
- No token required
- Bypasses all validation
- Proxied directly to upstream
- Used for public assets (thumbnails, icons, etc.)

### Album Shares (`/share/<uuid>`)

**First Request (Initial Validation):**
1. Extracts `?token=` from query string
2. Validates HMAC signature
3. Checks `exp` timestamp
4. Queries SQLite for `jti`:
   - If `revoked_at` is set → `401 Unauthorized`
   - If database is unreachable → `503 Service Unavailable` (fail closed)
5. Sets signed session cookie (24h TTL) containing album ID
6. **Strips token from query string** before forwarding
7. Proxies request to upstream

**Subsequent Requests (Images, Thumbnails):**
1. Checks for valid session cookie
2. If cookie exists and matches album ID → immediate proxy (no token required)
3. If no cookie → requires `?token=` parameter
4. Proxies request to upstream

This cookie-based approach allows album pages to load correctly with all embedded images, since immich-public-proxy doesn't add JWT tokens to image URLs.

### Fail-Closed Behavior

If the SQLite database is unreachable (corrupted, permissions issue, disk full), `imshare-verify` returns `503 Service Unavailable` rather than allowing unverified access.

## Testing

### Test 1: Expired Token

```bash
# Generate a link with 1-second TTL
export IMSHARE_SECRET="your-secret"
imshare generate abc-123 --ttl 1s

# Wait 2 seconds
sleep 2

# Try to access (should get 401)
curl -i "http://localhost:3001/share/abc-123?token=<generated-token>"
# Expected: HTTP/1.1 401 Unauthorized
```

### Test 2: Revoked Token

```bash
# Generate a link
imshare generate abc-123 --ttl 7d
# Note the ID, e.g., #5

# Revoke it
imshare revoke 5

# Try to access (should get 401)
curl -i "http://localhost:3001/share/abc-123?token=<generated-token>"
# Expected: HTTP/1.1 401 Unauthorized
# Response: Token has been revoked
```

### Test 3: Database Failure (Fail Closed)

```bash
# Make database temporarily unreachable
sudo chmod 000 ~/.local/share/imshare/links.db

# Try to access a valid link (should get 503)
curl -i "http://localhost:3001/share/abc-123?token=<valid-token>"
# Expected: HTTP/1.1 503 Service Unavailable
# Response: Database unavailable - failing closed

# Restore permissions
sudo chmod 644 ~/.local/share/imshare/links.db
```

## Configuration

Default config location: `~/.config/imshare/config.toml`

```toml
public_domain = "pub.nith.sh"
default_ttl = "30d"
db_path = "~/.local/share/imshare/links.db"
upstream = "http://localhost:3000"
verify_port = 3001
```

## Security Considerations

- **Secret Management**: Store `IMSHARE_SECRET` in `/etc/imshare/env`, never in version control
- **Database Permissions**: Ensure SQLite database is readable by `imshare-verify` user
- **Fail-Closed**: Middleware rejects requests if database is unreachable
- **Token Revocation**: Revoked tokens are checked on every request
- **No Token Reuse**: Extending a link issues a new JTI, invalidating the old URL

## Database Schema

```sql
CREATE TABLE links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    album_id TEXT NOT NULL,
    label TEXT,
    url TEXT NOT NULL,
    jti TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_at TEXT
);
```

## JWT Payload

```json
{
  "album_id": "abc-123-def",
  "exp": 1735689600,
  "jti": "550e8400-e29b-41d4-a716-446655440000"
}
```

- `exp` is omitted entirely for unlimited tokens
- `jti` (JWT ID) is a UUID v4 used for revocation tracking

## License

MIT
