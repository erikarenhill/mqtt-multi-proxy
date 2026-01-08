# MQTT Proxy API Documentation

Base URL: `http://localhost:3000`

## Endpoints

### Health Check

```http
GET /health
```

**Response**: `200 OK`
```
OK
```

---

### List All Brokers

```http
GET /api/brokers
```

**Response**: `200 OK`
```json
{
  "brokers": [
    {
      "id": "uuid-here",
      "name": "production",
      "address": "mqtt.example.com",
      "port": 8883,
      "client_id_prefix": "proxy-device",
      "username": "user",
      "password": "pass",
      "enabled": true,
      "use_tls": true,
      "insecure_skip_verify": false,
      "ca_cert_path": null
    }
  ]
}
```

---

### Get Single Broker

```http
GET /api/brokers/:id
```

**Response**: `200 OK`
```json
{
  "id": "uuid-here",
  "name": "production",
  "address": "mqtt.example.com",
  "port": 8883,
  "client_id_prefix": "proxy-device",
  "username": "user",
  "password": "pass",
  "enabled": true,
  "use_tls": true,
  "insecure_skip_verify": false,
  "ca_cert_path": null
}
```

**Errors**:
- `404 Not Found` - Broker not found

---

### Add New Broker

```http
POST /api/brokers
Content-Type: application/json
```

**Request Body**:
```json
{
  "name": "production",
  "address": "mqtt.example.com",
  "port": 8883,
  "clientIdPrefix": "proxy-device",
  "username": "user",
  "password": "pass",
  "enabled": true,
  "useTls": true,
  "insecureSkipVerify": false,
  "caCertPath": "/path/to/ca.crt"
}
```

**Fields**:
- `name` (required) - Unique broker name
- `address` (required) - Broker hostname or IP
- `port` (required) - Broker port (1-65535)
- `clientIdPrefix` (required) - Prefix for generating unique client IDs
- `username` (optional) - MQTT username
- `password` (optional) - MQTT password
- `enabled` (optional, default: true) - Enable broker immediately
- `useTls` (optional, default: false) - Use TLS/SSL
- `insecureSkipVerify` (optional, default: false) - Skip certificate verification
- `caCertPath` (optional) - Path to CA certificate

**Response**: `200 OK`
```json
{
  "id": "newly-generated-uuid",
  "name": "production",
  ...
}
```

**Errors**:
- `500 Internal Server Error` - Duplicate name, connection failed, etc.

---

### Update Broker

```http
PUT /api/brokers/:id
Content-Type: application/json
```

**Request Body**: Same as Add Broker (all fields required except username/password)

**Response**: `200 OK` - Updated broker object

**Errors**:
- `404 Not Found` - Broker not found
- `500 Internal Server Error` - Duplicate name, connection failed

**Note**: Updating a broker disconnects and reconnects with new settings.

---

### Delete Broker

```http
DELETE /api/brokers/:id
```

**Response**: `204 No Content`

**Errors**:
- `404 Not Found` - Broker not found

**Note**: Deletes broker from storage and disconnects immediately.

---

### Toggle Broker Enable/Disable

```http
POST /api/brokers/:id/toggle
Content-Type: application/json
```

**Request Body**:
```json
{
  "enabled": true
}
```

**Response**: `200 OK`

**Errors**:
- `404 Not Found` - Broker not found

**Effect**:
- `enabled: true` - Establishes connection to broker
- `enabled: false` - Disconnects from broker

---

### Get System Status

```http
GET /api/status
```

**Response**: `200 OK`
```json
{
  "brokers": [
    {
      "id": "uuid",
      "name": "production",
      "address": "mqtt.example.com",
      "port": 8883,
      "connected": true,
      "enabled": true
    }
  ],
  "total_messages_received": 1234,
  "total_messages_forwarded": 4936
}
```

---

## Error Format

All errors return JSON in this format:

```json
{
  "error": "Error message here"
}
```

**Status Codes**:
- `200 OK` - Success
- `204 No Content` - Success (DELETE)
- `404 Not Found` - Resource not found
- `500 Internal Server Error` - Server error

---

## Examples

### Add Broker with cURL

```bash
curl -X POST http://localhost:3000/api/brokers \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "my-broker",
    "address": "mqtt.example.com",
    "port": 1883,
    "clientIdPrefix": "proxy",
    "username": "myuser",
    "password": "mypass",
    "enabled": true,
    "useTls": false,
    "insecureSkipVerify": false
  }'
```

### List Brokers

```bash
curl http://localhost:3000/api/brokers
```

### Toggle Broker

```bash
curl -X POST http://localhost:3000/api/brokers/uuid-here/toggle \
  -H 'Content-Type: application/json' \
  -d '{"enabled": false}'
```

### Delete Broker

```bash
curl -X DELETE http://localhost:3000/api/brokers/uuid-here
```

---

## Integration

### JavaScript (Fetch API)

```javascript
// Add broker
const response = await fetch('/api/brokers', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    name: 'production',
    address: 'mqtt.example.com',
    port: 8883,
    clientIdPrefix: 'proxy',
    useTls: true,
  })
});

const broker = await response.json();
console.log('Added broker:', broker.id);

// List brokers
const brokers = await fetch('/api/brokers').then(r => r.json());
console.log(brokers.brokers);

// Delete broker
await fetch(`/api/brokers/${brokerId}`, { method: 'DELETE' });
```

### Python (requests)

```python
import requests

# Add broker
response = requests.post('http://localhost:3000/api/brokers', json={
    'name': 'production',
    'address': 'mqtt.example.com',
    'port': 8883,
    'clientIdPrefix': 'proxy',
    'useTls': True,
})
broker = response.json()
print(f"Added broker: {broker['id']}")

# List brokers
brokers = requests.get('http://localhost:3000/api/brokers').json()
for broker in brokers['brokers']:
    print(f"{broker['name']}: {broker['connected']}")

# Delete broker
requests.delete(f"http://localhost:3000/api/brokers/{broker_id}")
```

---

## Persistence

All broker configurations are stored in:
- **Docker**: `/app/data/brokers.json` (volume `mqtt-proxy-data`)
- **Local**: `./data/brokers.json`

Changes made via the API are **immediately persisted** and survive container restarts.
