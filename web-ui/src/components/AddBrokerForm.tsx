import { useState } from 'react'

interface BrokerFormData {
  name: string
  address: string
  port: number
  username?: string
  password?: string
  clientIdPrefix: string
  useTls: boolean
  insecureSkipVerify: boolean
  bidirectional: boolean
  topics: string[]
  subscriptionTopics: string[]
}

interface AddBrokerFormProps {
  onAdd: (broker: BrokerFormData) => void
  onCancel: () => void
  initialBroker?: BrokerFormData & { id?: string }
  isEditing?: boolean
}

export default function AddBrokerForm({ onAdd, onCancel, initialBroker, isEditing = false }: AddBrokerFormProps) {
  const [formData, setFormData] = useState<BrokerFormData>(initialBroker || {
    name: '',
    address: '',
    port: 1883,
    username: '',
    password: '',
    clientIdPrefix: 'proxy',
    useTls: false,
    insecureSkipVerify: false,
    bidirectional: false,
    topics: [],
    subscriptionTopics: [],
  })
  const [topicInput, setTopicInput] = useState('')
  const [subscriptionTopicInput, setSubscriptionTopicInput] = useState('')
  const [keepPassword, setKeepPassword] = useState(isEditing) // Default to true when editing

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    // If keeping password, don't send password field
    if (isEditing && keepPassword) {
      const { password: _password, ...dataWithoutPassword } = formData
      void _password // Intentionally unused - excluding password from submission
      onAdd(dataWithoutPassword as BrokerFormData)
    } else {
      onAdd(formData)
    }
  }

  const handleChange = (field: keyof BrokerFormData, value: string | number | boolean) => {
    setFormData(prev => ({ ...prev, [field]: value }))
  }

  const addTopic = () => {
    const topic = topicInput.trim()
    if (topic && !formData.topics.includes(topic)) {
      setFormData(prev => ({ ...prev, topics: [...prev.topics, topic] }))
      setTopicInput('')
    }
  }

  const removeTopic = (topicToRemove: string) => {
    setFormData(prev => ({
      ...prev,
      topics: prev.topics.filter(t => t !== topicToRemove)
    }))
  }

  const handleTopicKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      addTopic()
    }
  }

  const addSubscriptionTopic = () => {
    const topic = subscriptionTopicInput.trim()
    if (topic && !formData.subscriptionTopics.includes(topic)) {
      setFormData(prev => ({ ...prev, subscriptionTopics: [...prev.subscriptionTopics, topic] }))
      setSubscriptionTopicInput('')
    }
  }

  const removeSubscriptionTopic = (topicToRemove: string) => {
    setFormData(prev => ({
      ...prev,
      subscriptionTopics: prev.subscriptionTopics.filter(t => t !== topicToRemove)
    }))
  }

  const handleSubscriptionTopicKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      addSubscriptionTopic()
    }
  }

  return (
    <form className="add-broker-form" onSubmit={handleSubmit}>
      <h3>{isEditing ? 'Edit Broker Connection' : 'Add New Broker Connection'}</h3>

      <div className="form-group">
        <label htmlFor="name">Broker Name *</label>
        <input
          id="name"
          type="text"
          value={formData.name}
          onChange={(e) => handleChange('name', e.target.value)}
          placeholder="e.g., production, analytics"
          required
        />
      </div>

      <div className="form-row">
        <div className="form-group">
          <label htmlFor="address">IP Address / Hostname *</label>
          <input
            id="address"
            type="text"
            value={formData.address}
            onChange={(e) => handleChange('address', e.target.value)}
            placeholder="mqtt.example.com"
            required
          />
        </div>

        <div className="form-group">
          <label htmlFor="port">Port *</label>
          <input
            id="port"
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
        <label htmlFor="clientIdPrefix">Client ID Prefix *</label>
        <input
          id="clientIdPrefix"
          type="text"
          value={formData.clientIdPrefix}
          onChange={(e) => handleChange('clientIdPrefix', e.target.value)}
          placeholder="proxy-device"
          required
        />
        <small>Used to generate unique client IDs for this broker</small>
      </div>

      <div className="form-row">
        <div className="form-group">
          <label htmlFor="username">Username (optional)</label>
          <input
            id="username"
            type="text"
            value={formData.username || ''}
            onChange={(e) => handleChange('username', e.target.value)}
            placeholder="Leave empty if no auth"
          />
        </div>

        <div className="form-group">
          <label htmlFor="password">Password (optional)</label>
          {isEditing && (
            <label className="checkbox-label" style={{ marginBottom: '0.5rem' }}>
              <input
                type="checkbox"
                checked={keepPassword}
                onChange={(e) => setKeepPassword(e.target.checked)}
              />
              <span>Keep current password</span>
            </label>
          )}
          {(!isEditing || !keepPassword) && (
            <input
              id="password"
              type="password"
              value={formData.password || ''}
              onChange={(e) => handleChange('password', e.target.value)}
              placeholder={isEditing ? "Enter new password" : "Leave empty if no auth"}
            />
          )}
        </div>
      </div>

      <div className="form-group">
        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={formData.useTls}
            onChange={(e) => handleChange('useTls', e.target.checked)}
          />
          <span>Use TLS/SSL</span>
        </label>
      </div>

      {formData.useTls && (
        <div className="form-group tls-options">
          <label className="checkbox-label">
            <input
              type="checkbox"
              checked={formData.insecureSkipVerify}
              onChange={(e) => handleChange('insecureSkipVerify', e.target.checked)}
            />
            <span>Skip certificate verification (self-signed certificates)</span>
          </label>
          <small className="warning-text">
            ⚠️ Warning: Only enable this for self-signed certificates. Disabling verification
            reduces security.
          </small>
        </div>
      )}

      <div className="form-group">
        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={formData.bidirectional}
            onChange={(e) => handleChange('bidirectional', e.target.checked)}
          />
          <span>Enable Bidirectional Message Forwarding</span>
        </label>
        <small>
          When enabled, messages published to this broker will be forwarded back to subscribed clients.
          Use this for brokers like Home Assistant where you want to receive messages back.
        </small>
      </div>

      <div className="form-group">
        <label htmlFor="topics">Topic Filters (optional)</label>
        <div className="topic-input-wrapper">
          <input
            id="topics"
            type="text"
            value={topicInput}
            onChange={(e) => setTopicInput(e.target.value)}
            onKeyPress={handleTopicKeyPress}
            placeholder="e.g., sensor/# or home/+/temperature"
          />
          <button type="button" className="btn-add-topic" onClick={addTopic}>
            Add
          </button>
        </div>
        <small>
          Specify which topics this broker should receive. Leave empty to forward all messages.
          Supports MQTT wildcards: <strong>+</strong> (single level) and <strong>#</strong> (multi-level).
        </small>
        {formData.topics.length > 0 && (
          <div className="topic-chips">
            {formData.topics.map((topic) => (
              <div key={topic} className="topic-chip">
                <span>{topic}</span>
                <button
                  type="button"
                  className="remove-chip"
                  onClick={() => removeTopic(topic)}
                  aria-label={`Remove ${topic}`}
                >
                  ×
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {formData.bidirectional && (
        <div className="form-group">
          <label htmlFor="subscriptionTopics">Subscription Topics (optional)</label>
          <div className="topic-input-wrapper">
            <input
              id="subscriptionTopics"
              type="text"
              value={subscriptionTopicInput}
              onChange={(e) => setSubscriptionTopicInput(e.target.value)}
              onKeyPress={handleSubscriptionTopicKeyPress}
              placeholder="e.g., homeassistant/# or zigbee2mqtt/#"
            />
            <button type="button" className="btn-add-topic" onClick={addSubscriptionTopic}>
              Add
            </button>
          </div>
          <small>
            Topics to subscribe to on this broker. If empty, uses the topic filters above.
            Use this to receive different topics than what you filter for forwarding.
          </small>
          {formData.subscriptionTopics.length > 0 && (
            <div className="topic-chips">
              {formData.subscriptionTopics.map((topic) => (
                <div key={topic} className="topic-chip">
                  <span>{topic}</span>
                  <button
                    type="button"
                    className="remove-chip"
                    onClick={() => removeSubscriptionTopic(topic)}
                    aria-label={`Remove ${topic}`}
                  >
                    ×
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      <div className="form-actions">
        <button type="button" className="btn-secondary" onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" className="btn-primary">
          {isEditing ? 'Update Broker' : 'Add Broker'}
        </button>
      </div>
    </form>
  )
}
