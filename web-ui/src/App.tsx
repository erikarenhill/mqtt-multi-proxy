import { useEffect, useState } from 'react'
import BrokerList from './components/BrokerList'
import MetricsDashboard from './components/MetricsDashboard'
import AddBrokerForm from './components/AddBrokerForm'
import MessageViewer from './components/MessageViewer'
import MainBrokerSettings from './components/MainBrokerSettings'
import './App.css'

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
  subscriptionTopics: string[]
}

interface BrokerStatus {
  id: string
  connected: boolean
}

interface BrokerFormData {
  name: string
  address: string
  port: number
  clientIdPrefix: string
  enabled?: boolean
  username?: string
  password?: string
  useTls?: boolean
  insecureSkipVerify?: boolean
  bidirectional?: boolean
  topics?: string[]
  subscriptionTopics?: string[]
}

function App() {
  const [brokers, setBrokers] = useState<Broker[]>([])
  const [loading, setLoading] = useState(true)
  const [showAddForm, setShowAddForm] = useState(false)
  const [editingBroker, setEditingBroker] = useState<Broker | null>(null)

  useEffect(() => {
    fetchBrokers()
    const interval = setInterval(fetchBrokers, 5000) // Poll every 5 seconds
    return () => clearInterval(interval)
  }, [])

  const fetchBrokers = async () => {
    try {
      // Fetch full broker configs from /api/brokers
      const brokersResponse = await fetch('/api/brokers')
      const brokersData = await brokersResponse.json()

      // Fetch status from /api/status to get connected state
      const statusResponse = await fetch('/api/status')
      const statusData = await statusResponse.json()

      // Merge the data - add connected state from status to broker configs
      // Also map snake_case API response to camelCase for frontend
      const brokersWithStatus = brokersData.brokers.map((broker: Broker & { subscription_topics?: string[] }) => {
        const status = statusData.brokers.find((s: BrokerStatus) => s.id === broker.id)
        return {
          ...broker,
          subscriptionTopics: broker.subscription_topics || broker.subscriptionTopics || [],
          connected: status?.connected || false,
        }
      })

      setBrokers(brokersWithStatus)
      setLoading(false)
    } catch (error) {
      console.error('Failed to fetch brokers:', error)
      setLoading(false)
    }
  }

  const handleAddBroker = async (brokerData: BrokerFormData) => {
    try {
      const response = await fetch('/api/brokers', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(brokerData),
      })

      if (response.ok) {
        setShowAddForm(false)
        fetchBrokers() // Refresh the list
      } else {
        const error = await response.json()
        alert(`Failed to add broker: ${error.error}`)
      }
    } catch (error) {
      console.error('Error adding broker:', error)
      alert('Network error: Failed to add broker')
    }
  }

  const handleDeleteBroker = async (id: string) => {
    if (!confirm('Are you sure you want to delete this broker?')) {
      return
    }

    try {
      const response = await fetch(`/api/brokers/${id}`, {
        method: 'DELETE',
      })

      if (response.ok) {
        fetchBrokers()
      } else {
        alert('Failed to delete broker')
      }
    } catch (error) {
      console.error('Error deleting broker:', error)
    }
  }

  const handleToggleBroker = async (id: string, enabled: boolean) => {
    try {
      const response = await fetch(`/api/brokers/${id}/toggle`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled }),
      })

      if (response.ok) {
        fetchBrokers()
      } else {
        alert('Failed to toggle broker')
      }
    } catch (error) {
      console.error('Error toggling broker:', error)
    }
  }

  const handleEditBroker = (broker: Broker) => {
    // Transform broker to match form data structure
    setEditingBroker({
      id: broker.id,
      name: broker.name,
      address: broker.address,
      port: broker.port,
      clientIdPrefix: broker.clientIdPrefix,
      username: broker.username || '',
      password: '', // Password not returned from API, leave empty unless user wants to change
      useTls: broker.useTls,
      insecureSkipVerify: broker.insecureSkipVerify,
      bidirectional: broker.bidirectional,
      topics: broker.topics,
      subscriptionTopics: broker.subscriptionTopics || [],
      connected: broker.connected,
      enabled: broker.enabled,
    })
  }

  const handleUpdateBroker = async (brokerData: BrokerFormData) => {
    if (!editingBroker) return

    try {
      // Ensure all required fields are present
      const updateData: Record<string, unknown> = {
        name: brokerData.name,
        address: brokerData.address,
        port: brokerData.port,
        clientIdPrefix: brokerData.clientIdPrefix,
        enabled: brokerData.enabled,
        useTls: brokerData.useTls || false,
        insecureSkipVerify: brokerData.insecureSkipVerify || false,
        bidirectional: brokerData.bidirectional || false,
        topics: brokerData.topics || [],
        subscriptionTopics: brokerData.subscriptionTopics || [],
      }

      // Only include username/password if they have values
      if (brokerData.username && brokerData.username.trim() !== '') {
        updateData.username = brokerData.username
      }
      if (brokerData.password && brokerData.password.trim() !== '') {
        updateData.password = brokerData.password
      }

      const response = await fetch(`/api/brokers/${editingBroker.id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(updateData),
      })

      if (response.ok) {
        setEditingBroker(null)
        fetchBrokers()
      } else {
        const error = await response.json()
        alert(`Failed to update broker: ${error.error}`)
      }
    } catch (error) {
      console.error('Error updating broker:', error)
      alert('Network error: Failed to update broker')
    }
  }

  return (
    <div className="app">
      <header>
        <h1>MQTT Proxy Dashboard</h1>
        <p>Real-time monitoring and control</p>
      </header>

      <main>
        <section className="main-broker-section">
          <MainBrokerSettings />
        </section>

        <section className="brokers-section">
          <div className="section-header">
            <h2>Connected Brokers</h2>
            <button
              className="btn-primary"
              onClick={() => setShowAddForm(true)}
            >
              + Add Broker
            </button>
          </div>

          {showAddForm && (
            <div className="modal-overlay">
              <div className="modal-content">
                <AddBrokerForm
                  onAdd={handleAddBroker}
                  onCancel={() => setShowAddForm(false)}
                />
              </div>
            </div>
          )}

          {editingBroker && (
            <div className="modal-overlay">
              <div className="modal-content">
                <AddBrokerForm
                  onAdd={handleUpdateBroker}
                  onCancel={() => setEditingBroker(null)}
                  initialBroker={editingBroker}
                  isEditing={true}
                />
              </div>
            </div>
          )}

          {loading ? (
            <p>Loading...</p>
          ) : (
            <BrokerList
              brokers={brokers}
              onDelete={handleDeleteBroker}
              onToggle={handleToggleBroker}
              onEdit={handleEditBroker}
            />
          )}
        </section>

        <section className="metrics-section">
          <h2>Performance Metrics</h2>
          <MetricsDashboard />
        </section>

        <section className="messages-section">
          <MessageViewer />
        </section>
      </main>
    </div>
  )
}

export default App
