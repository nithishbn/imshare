# imshare - Temporary Public Links for Immich via immich-public-proxy

## What is imshare?

imshare creates temporary, revocable public share links for Immich albums using immich-public-proxy.

### The Problem It Solves

Immich already has share links with expiry dates, but if you want to share albums publicly:
- You either expose your server's IP directly
- OR require friends to install Tailscale/authenticate
- OR put everything behind Cloudflare Access (but then it's not truly public)

### The Solution

imshare uses **immich-public-proxy** for read-only access and adds JWT-based temporary access control:
- Friends get a public link (no Tailscale, no auth required)
- Your server IP stays hidden (behind Cloudflare Tunnel)
- Links are temporary and revocable
- Read-only access only (via immich-public-proxy)

## Architecture

You run TWO domains:
1. **img.nith.sh** - Your private Immich (protected by Cloudflare Access)
2. **pub.nith.sh** - Public temporary shares (no auth, JWT-validated)

### How it works:

```
Friend clicks: pub.nith.sh/share/abc-123?token=jwt...
    ↓
imshare-verify validates JWT (expired? revoked?)
    ↓
If valid → forwards to immich-public-proxy (read-only)
If invalid → 403 Forbidden
```

## Components

1. **imshare CLI** - Generate/manage temporary links
2. **imshare-api** - HTTP API for automation (iOS Shortcuts, etc.)
3. **imshare-verify** - JWT validation proxy before immich-public-proxy
4. **immich-public-proxy** - Read-only Immich viewer (separate project)

## Installation

### Prerequisites
- Immich running
- immich-public-proxy set up
- Two domains (one for private access, one for public)
- Cloudflare Tunnel (to hide your IP)

### Setup Steps

1. **Generate a secret key:**
```bash
openssl rand -base64 32
```

2. **Create config** at `~/.config/imshare/config.toml`:
```toml
# Public domain for temporary shares (no auth)
public_domain = "pub.example.com"

# Default expiration
default_ttl = "30d"

# Database path
db_path = "~/.local/share/imshare/links.db"

# immich-public-proxy URL
upstream = "http://localhost:3000"

# Port for verification proxy
verify_port = 3001
```

3. **Set environment variable:**
```bash
export IMSHARE_SECRET="your-secret-from-step-1"
```

4. **Run services:**
```bash
# JWT verification proxy
imshare-verify

# API server (optional)
IMSHARE_API_PORT=3002 imshare-api
```

5. **Configure routing:**
```
img.example.com → Immich (protected by Cloudflare Access)
pub.example.com → imshare-verify → immich-public-proxy
```

## Usage

### CLI

**Generate a temporary public link:**
```bash
# From Immich share URL
imshare generate https://img.example.com/share/abc-123

# With custom expiration
imshare generate abc-123 --ttl 7d --label "Vacation Photos"

# Returns:
# - Public URL: https://pub.example.com/share/abc-123?token=...
# - QR code (as ASCII art + base64 PNG)
```

**List links:**
```bash
imshare list
```

**Revoke a link:**
```bash
imshare revoke --id 5
```

**Extend expiration:**
```bash
imshare extend --id 5 --ttl 14d
```

### HTTP API

**Generate link:**
```bash
curl -X POST https://img.example.com/imshare-api/generate \
  -H "Content-Type: application/json" \
  -d '{
    "album_id": "abc-123",
    "ttl": "7d",
    "label": "Vacation Photos"
  }'
```

**List links:**
```bash
curl https://img.example.com/imshare-api/list
```

**Revoke link:**
```bash
curl -X POST https://img.example.com/imshare-api/revoke \
  -H "Content-Type: application/json" \
  -d '{"id": 1}'
```

**Extend link:**
```bash
curl -X POST https://img.example.com/imshare-api/extend \
  -H "Content-Type: application/json" \
  -d '{"id": 1, "ttl": "14d"}'
```

## iOS Shortcut

Create a Shortcut that:
1. Takes Immich share link from share sheet
2. Calls imshare API to generate temporary public link
3. Returns link you can send to friends (no auth needed!)

Perfect for: "Hey, here are photos from last night!" without making friends create accounts.

## Why Two Domains?

- **img.example.com** - Your private access (Cloudflare Access auth required)
- **pub.example.com** - Temporary public shares (no auth, just JWT validation)

This way:
- Friends don't need accounts/Tailscale
- Your IP stays hidden (Cloudflare Tunnel)
- You control expiration and can revoke anytime
- Read-only access via immich-public-proxy (can't modify/delete)

## Security

- JWTs embed album ID + expiration + unique ID
- Every request validates: expired? revoked?
- immich-public-proxy ensures read-only access
- Your server IP never exposed (Cloudflare Tunnel)
- `IMSHARE_SECRET` signs all tokens

## TTL Format

- `7d` - 7 days
- `24h` - 24 hours
- `30d` - 30 days
- `1w` - 1 week
- `never` - no expiration

## Questions?

This system lets you share Immich albums publicly without exposing your server or requiring friends to authenticate, while maintaining full control over access.
