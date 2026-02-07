import { useEffect, useState } from 'react'

interface MainBrokerFormData {
  address: string
  port: number
  clientId: string
  username: string
  password: string
}

interface TestResult {
  success: boolean
  message: string
  latencyMs?: number
}

export default function MainBrokerSettings() {
  const [formData, setFormData] = useState<MainBrokerFormData>({
    address: 'mosquitto',
    port: 1883,
    clientId: 'mqtt-proxy',
    username: '',
    password: '',
  })
  const [keepPassword, setKeepPassword] = useState(false)
  const [hasExistingPassword, setHasExistingPassword] = useState(false)
  const [testResult, setTestResult] = useState<TestResult | null>(null)
  const [testing, setTesting] = useState(false)
  const [saving, setSaving] = useState(false)
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    fetchSettings()
  }, [])

  const fetchSettings = async () => {
    try {
      const response = await fetch('/api/settings/main-broker')
      const data = await response.json()
      if (data.settings) {
        const s = data.settings
        const hasPwd = s.password && s.password !== ''
        setHasExistingPassword(hasPwd)
        setKeepPassword(hasPwd)
        setFormData({
          address: s.address || 'mosquitto',
          port: s.port || 1883,
          clientId: s.clientId || 'mqtt-proxy',
          username: s.username || '',
          password: '',
        })
      }
      setLoaded(true)
    } catch (error) {
      console.error('Failed to fetch main broker settings:', error)
      setLoaded(true)
    }
  }

  const handleChange = (field: keyof MainBrokerFormData, value: string | number) => {
    setFormData(prev => ({ ...prev, [field]: value }))
    setTestResult(null) // Reset test result on any change
  }

  const handleTestConnection = async () => {
    setTesting(true)
    setTestResult(null)
    try {
      const payload: Record<string, unknown> = {
        address: formData.address,
        port: formData.port,
        clientId: formData.clientId,
      }
      if (formData.username) payload.username = formData.username
      if (!keepPassword && formData.password) payload.password = formData.password

      const response = await fetch('/api/settings/main-broker/test', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      })
      const result = await response.json()
      setTestResult(result)
    } catch (error) {
      setTestResult({
        success: false,
        message: 'Network error: could not reach the proxy server',
      })
    } finally {
      setTesting(false)
    }
  }

  const handleSave = async () => {
    setSaving(true)
    try {
      const payload: Record<string, unknown> = {
        address: formData.address,
        port: formData.port,
        clientId: formData.clientId,
      }
      if (formData.username) payload.username = formData.username

      if (keepPassword && hasExistingPassword) {
        payload.password = '********'
      } else if (formData.password) {
        payload.password = formData.password
      }

      const response = await fetch('/api/settings/main-broker', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      })

      if (response.ok) {
        setTestResult({
          success: true,
          message: 'Settings saved! Main broker is reconnecting...',
        })
        // Refresh to show saved state
        await fetchSettings()
      } else {
        const error = await response.json()
        setTestResult({
          success: false,
          message: `Failed to save: ${error.error}`,
        })
      }
    } catch (error) {
      setTestResult({
        success: false,
        message: 'Network error: Failed to save settings',
      })
    } finally {
      setSaving(false)
    }
  }

  if (!loaded) return null

  const canSave = testResult?.success === true && !saving

  return (
    <div className="main-broker-settings">
      <h3>Main Broker Connection</h3>
      <p className="settings-description">
        Configure the primary MQTT broker that this proxy connects to for receiving messages.
      </p>

      <div className="form-row">
        <div className="form-group">
          <label htmlFor="mb-address">Address *</label>
          <input
            id="mb-address"
            type="text"
            value={formData.address}
            onChange={(e) => handleChange('address', e.target.value)}
            placeholder="mosquitto"
            required
          />
        </div>
        <div className="form-group">
          <label htmlFor="mb-port">Port *</label>
          <input
            id="mb-port"
            type="number"
            value={formData.port}
            onChange={(e) => handleChange('port', parseInt(e.target.value))}
            min="1"
            max="65535"
            required
          />
        </div>
      </div>

      <div className="form-group">
        <label htmlFor="mb-clientId">Client ID</label>
        <input
          id="mb-clientId"
          type="text"
          value={formData.clientId}
          onChange={(e) => handleChange('clientId', e.target.value)}
          placeholder="mqtt-proxy"
        />
      </div>

      <div className="form-row">
        <div className="form-group">
          <label htmlFor="mb-username">Username (optional)</label>
          <input
            id="mb-username"
            type="text"
            value={formData.username}
            onChange={(e) => handleChange('username', e.target.value)}
            placeholder="Leave empty if no auth"
          />
        </div>
        <div className="form-group">
          <label htmlFor="mb-password">Password (optional)</label>
          {hasExistingPassword && (
            <label className="checkbox-label" style={{ marginBottom: '0.5rem' }}>
              <input
                type="checkbox"
                checked={keepPassword}
                onChange={(e) => {
                  setKeepPassword(e.target.checked)
                  setTestResult(null)
                }}
              />
              <span>Keep current password</span>
            </label>
          )}
          {!keepPassword && (
            <input
              id="mb-password"
              type="password"
              value={formData.password}
              onChange={(e) => handleChange('password', e.target.value)}
              placeholder="Leave empty if no auth"
            />
          )}
        </div>
      </div>

      <div className="settings-actions">
        <button
          type="button"
          className="btn-test"
          onClick={handleTestConnection}
          disabled={testing || !formData.address}
        >
          {testing ? 'Testing...' : 'Test Connection'}
        </button>
        <button
          type="button"
          className="btn-primary"
          onClick={handleSave}
          disabled={!canSave}
        >
          {saving ? 'Saving...' : 'Save & Reconnect'}
        </button>
      </div>

      {testResult && (
        <div className={`test-result ${testResult.success ? 'success' : 'error'}`}>
          <span>{testResult.message}</span>
          {testResult.latencyMs !== undefined && (
            <span className="latency"> ({testResult.latencyMs}ms)</span>
          )}
        </div>
      )}
    </div>
  )
}
