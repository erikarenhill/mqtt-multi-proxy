interface Broker {
  id: string
  name: string
  address: string
  port: number
  connected: boolean
  enabled: boolean
  clientIdPrefix: string
  username?: string
  password?: string
  useTls: boolean
  insecureSkipVerify: boolean
  bidirectional: boolean
  topics: string[]
}

interface BrokerListProps {
  brokers: Broker[]
  onDelete: (id: string) => void
  onToggle: (id: string, enabled: boolean) => void
  onEdit: (broker: Broker) => void
}

export default function BrokerList({ brokers, onDelete, onToggle, onEdit }: BrokerListProps) {
  return (
    <div className="broker-list">
      {brokers.length === 0 ? (
        <div className="empty-state">
          <p>No brokers configured yet</p>
          <p className="hint">Click "Add Broker" to connect to your first MQTT broker</p>
        </div>
      ) : (
        <div className="broker-grid">
          {brokers.map((broker) => (
            <div
              key={broker.id}
              className={`broker-card ${broker.connected ? 'connected' : 'disconnected'} ${
                !broker.enabled ? 'disabled' : ''
              }`}
            >
              <div className="broker-header">
                <div className="broker-status">
                  <span
                    className={`status-indicator ${
                      broker.enabled
                        ? broker.connected
                          ? 'online'
                          : 'offline'
                        : 'disabled'
                    }`}
                  ></span>
                  <h3>{broker.name}</h3>
                </div>
                <div className="broker-actions">
                  <button
                    className="icon-btn"
                    onClick={() => onEdit(broker)}
                    title="Edit broker"
                  >
                    ✏
                  </button>
                  <button
                    className="icon-btn"
                    onClick={() => onToggle(broker.id, !broker.enabled)}
                    title={broker.enabled ? 'Disable broker' : 'Enable broker'}
                  >
                    {broker.enabled ? '⏸' : '▶'}
                  </button>
                  <button
                    className="icon-btn delete"
                    onClick={() => onDelete(broker.id)}
                    title="Delete broker"
                  >
                    ×
                  </button>
                </div>
              </div>

              <div className="broker-details">
                <p>
                  <strong>Address:</strong> {broker.address}:{broker.port}
                </p>
                <p>
                  <strong>Status:</strong>{' '}
                  {broker.enabled
                    ? broker.connected
                      ? 'Connected'
                      : 'Disconnected'
                    : 'Disabled'}
                </p>
                {broker.bidirectional && (
                  <p>
                    <strong>Mode:</strong> Bidirectional
                  </p>
                )}
                {broker.topics && broker.topics.length > 0 && (
                  <div>
                    <p><strong>Topics:</strong></p>
                    <div className="topic-chips">
                      {broker.topics.map((topic) => (
                        <div key={topic} className="topic-chip-small">
                          {topic}
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
