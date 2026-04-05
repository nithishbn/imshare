# imshare API Server

A lightweight HTTP API for generating share links - perfect for iOS Shortcuts, automation, and integrations.

## Quick Start

### Running the API Server

```bash
export IMSHARE_SECRET="your-secret-here"
export IMSHARE_API_PORT=3002  # Optional, defaults to 3002
./imshare-api
```

Or add it as a systemd service (see NixOS configuration below).

## API Endpoints

### POST /imshare-api/generate

Generate a new share link with QR code.

**Request:**
```bash
curl -X POST http://localhost:3002/imshare-api/generate \
  -H "Content-Type: application/json" \
  -d '{
    "album_id": "abc-123-def-456",
    "ttl": "7d",
    "label": "Beach Photos"
  }'
```

**Request Body:**
```json
{
  "album_id": "abc-123-def-456",  // Required: Album UUID or full Immich share URL
  "ttl": "7d",                     // Optional: Time-to-live (7d, 24h, 1w, unlimited)
  "label": "Beach Photos"          // Optional: Human-readable label
}
```

**Response:**
```json
{
  "id": 42,
  "url": "https://pub.nith.sh/share/abc-123?token=eyJ...",
  "qr_code_png_base64": "iVBORw0KGgoAAAANSUhEUgAA...",
  "album_id": "abc-123-def-456",
  "expires_at": "2026-04-10T07:00:00Z"
}
```

The `qr_code_png_base64` field contains a base64-encoded PNG image of the QR code.

### GET /imshare-api/list

List all share links.

**Request:**
```bash
curl http://localhost:3002/imshare-api/list
```

**Response:**
```json
{
  "links": [
    {
      "id": 1,
      "label": "Beach Photos",
      "album_id": "abc-123",
      "url": "https://pub.nith.sh/share/abc-123?token=eyJ...",
      "expires_at": "2026-04-10T07:00:00Z",
      "status": "Active"
    },
    {
      "id": 2,
      "label": "Vacation 2024",
      "album_id": "def-456",
      "url": "https://pub.nith.sh/share/def-456?token=eyJ...",
      "expires_at": null,
      "status": "Active"
    }
  ]
}
```

### POST /imshare-api/revoke

Revoke a share link by ID.

**Request:**
```bash
curl -X POST http://localhost:3002/imshare-api/revoke \
  -H "Content-Type: application/json" \
  -d '{"id": 5}'
```

**Request Body:**
```json
{
  "id": 5  // Required: Link ID to revoke
}
```

**Response:**
```json
{
  "success": true,
  "message": "Link 5 revoked successfully"
}
```

### POST /imshare-api/extend

Extend the expiration of an existing link with a new TTL.

**Request:**
```bash
curl -X POST http://localhost:3002/imshare-api/extend \
  -H "Content-Type: application/json" \
  -d '{
    "id": 5,
    "ttl": "14d"
  }'
```

**Request Body:**
```json
{
  "id": 5,        // Required: Link ID to extend
  "ttl": "14d"    // Required: New time-to-live
}
```

**Response:**
```json
{
  "id": 5,
  "url": "https://pub.nith.sh/share/abc-123?token=eyJ...",
  "qr_code_png_base64": "iVBORw0KGgoAAAANSUhEUgAA...",
  "expires_at": "2026-04-18T07:00:00Z"
}
```

**⚠️ Important:** Extending a link generates a new JWT with a new JTI, which invalidates the old URL. You must share the new URL with recipients.

### GET /imshare-api/health

Health check endpoint.

**Response:**
```json
{
  "status": "ok"
}
```

## iOS Shortcuts Integration

### Step 1: Create Shortcut

1. Open **Shortcuts** app on iPhone
2. Create new shortcut
3. Add **Share Sheet** as input (select Photos)
4. Add **Get Contents of URL** action

### Step 2: Configure Request

**URL:** `http://your-server:3002/imshare-api/generate`

**Method:** POST

**Headers:**
- `Content-Type: application/json`

**Request Body:**
```
{
  "album_id": "[Shortcut Input]",
  "ttl": "7d",
  "label": "Shared from iPhone"
}
```

### Step 3: Process Response

1. Add **Get Dictionary from Input** action
2. Add **Get Dictionary Value** for key `url`
3. Add **Get Dictionary Value** for key `qr_code_png_base64`
4. Add **Base64 Decode** on the QR code
5. Add **Show Result** or **Share**

### Example Shortcut Flow

```
Share Sheet (Get selected photos)
  ↓
Get Album ID from Immich Share URL
  ↓
Text (Build JSON):
{
  "album_id": [Album ID],
  "ttl": "7d",
  "label": "iPhone Share"
}
  ↓
Get Contents of URL
  URL: http://your-server:3002/imshare-api/generate
  Method: POST
  Headers: Content-Type: application/json
  Request Body: [Previous Result]
  ↓
Get Dictionary from Input
  ↓
Get value for "url" → Copy to Clipboard
Get value for "qr_code_png_base64" → Base64 Decode → Show as Image
  ↓
Show Notification: "Share link copied!"
```

## Advanced Usage

### Generate Link with QR Code (One-liner)

```bash
curl -X POST http://localhost:3002/imshare-api/generate \
  -H "Content-Type: application/json" \
  -d '{"album_id":"abc-123","ttl":"7d"}' \
  | jq -r '.qr_code_png_base64' \
  | base64 -d > qr.png

# Open QR code
open qr.png  # macOS
xdg-open qr.png  # Linux
```

### Get Just the URL

```bash
curl -s -X POST http://localhost:3002/imshare-api/generate \
  -H "Content-Type: application/json" \
  -d '{"album_id":"abc-123"}' \
  | jq -r '.url'
```

### Check Active Links

```bash
curl -s http://localhost:3002/imshare-api/list \
  | jq '.links[] | select(.status == "Active")'
```

## NixOS Integration

Add the API server to your NixOS configuration:

```nix
# In configuration.nix

systemd.services.imshare-api = {
  description = "imshare HTTP API server";
  wantedBy = [ "multi-user.target" ];
  after = [ "network.target" ];

  serviceConfig = {
    Type = "simple";
    User = "imshare";
    Group = "imshare";
    ExecStart = "${imshare}/bin/imshare-api";
    Restart = "always";
    RestartSec = "10s";
    EnvironmentFile = "/etc/imshare/env";
    WorkingDirectory = "/var/lib/imshare";

    # Hardening
    NoNewPrivileges = true;
    PrivateTmp = true;
    ProtectSystem = "strict";
    ProtectHome = true;
    ReadWritePaths = [ "/var/lib/imshare" ];
  };

  environment = {
    IMSHARE_API_PORT = "3002";
  };
};
```

### Expose via Tailscale

```nix
# In your Caddy config
virtualHosts."photos.tail17ebfc.ts.net".extraConfig = ''
  reverse_proxy /imshare-api/* localhost:3002
'';
```

## Security Considerations

### Authentication

The current API has **no authentication**. Recommendations:

1. **Only expose via Tailscale** - Don't expose port 3002 publicly
2. **Use Caddy basic auth** if you need external access:
   ```
   basicauth /imshare-api/* {
     username $2a$14$hash...
   }
   ```
3. **Add API key authentication** (future enhancement)

### Rate Limiting

Consider adding rate limiting via Caddy or nginx:

```nginx
limit_req_zone $binary_remote_addr zone=api:10m rate=10r/m;

location /imshare-api/ {
    limit_req zone=api burst=5;
    proxy_pass http://localhost:3002;
}
```

## Troubleshooting

### API won't start

```bash
# Check logs
sudo journalctl -u imshare-api -f

# Common issues:
# - IMSHARE_SECRET not set
# - Port 3002 already in use
# - Database not accessible
```

### iOS Shortcut not working

1. Check the URL is accessible from your phone (use Tailscale)
2. Verify the JSON format is correct
3. Check API logs for errors
4. Test with curl first before using Shortcuts

### QR code won't decode

The base64 string is a PNG image. Make sure to:
1. Use **Base64 Decode** action in Shortcuts
2. Use `base64 -d` in terminal (not `-D`)
3. Save as `.png` file

## Examples

### Python Script

```python
import requests
import base64
from PIL import Image
from io import BytesIO

response = requests.post('http://localhost:3002/api/generate', json={
    'album_id': 'abc-123-def-456',
    'ttl': '7d',
    'label': 'Python Generated'
})

data = response.json()
print(f"URL: {data['url']}")

# Display QR code
qr_bytes = base64.b64decode(data['qr_code_png_base64'])
qr_image = Image.open(BytesIO(qr_bytes))
qr_image.show()
```

### JavaScript/Node.js

```javascript
const response = await fetch('http://localhost:3002/api/generate', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    album_id: 'abc-123-def-456',
    ttl: '7d',
    label: 'JS Generated'
  })
});

const data = await response.json();
console.log('URL:', data.url);

// Save QR code
const qrBuffer = Buffer.from(data.qr_code_png_base64, 'base64');
require('fs').writeFileSync('qr.png', qrBuffer);
```

## Future Enhancements

Potential additions:
- [ ] API key authentication
- [ ] Webhook notifications when links expire
- [x] Revoke/extend endpoints
- [ ] Usage statistics
- [ ] Custom QR code styling (colors, logo)
- [ ] SVG QR codes
- [ ] Batch link generation

## Summary

The imshare API provides:
- ✅ Simple HTTP API for link generation
- ✅ Base64-encoded QR codes
- ✅ Perfect for iOS Shortcuts
- ✅ Easy to integrate with any platform
- ✅ Shares database with CLI tool
- ✅ No external dependencies

Access it on port 3002 and integrate with your workflows!
