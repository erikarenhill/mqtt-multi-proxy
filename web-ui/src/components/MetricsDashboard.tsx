import { useEffect, useState } from 'react'

interface Metrics {
  messagesReceived: number
  messagesForwarded: number
  avgLatencyMs: number
  activeConnections: number
}

export default function MetricsDashboard() {
  const [metrics, setMetrics] = useState<Metrics>({
    messagesReceived: 0,
    messagesForwarded: 0,
    avgLatencyMs: 0,
    activeConnections: 0,
  })

  useEffect(() => {
    fetchMetrics()
    const interval = setInterval(fetchMetrics, 2000)
    return () => clearInterval(interval)
  }, [])

  const fetchMetrics = async () => {
    try {
      const response = await fetch('/api/status')
      const data = await response.json()

      setMetrics({
        messagesReceived: data.total_messages_received || 0,
        messagesForwarded: data.total_messages_forwarded || 0,
        avgLatencyMs: data.avg_latency_ms || 0,
        activeConnections: data.brokers?.filter((b: any) => b.connected && b.enabled).length || 0,
      })
    } catch (error) {
      console.error('Failed to fetch metrics:', error)
    }
  }

  return (
    <div className="metrics-grid">
      <div className="metric-card">
        <h4>Messages Received</h4>
        <p className="metric-value">{metrics.messagesReceived.toLocaleString()}</p>
        <p className="metric-label">Total from devices</p>
      </div>
      <div className="metric-card">
        <h4>Messages Forwarded</h4>
        <p className="metric-value">{metrics.messagesForwarded.toLocaleString()}</p>
        <p className="metric-label">Total to brokers</p>
      </div>
      <div className="metric-card">
        <h4>Avg Latency</h4>
        <p className="metric-value">{metrics.avgLatencyMs.toFixed(2)} ms</p>
        <p className="metric-label">Message forwarding time</p>
      </div>
      <div className="metric-card">
        <h4>Active Brokers</h4>
        <p className="metric-value">{metrics.activeConnections}</p>
        <p className="metric-label">Connected and enabled</p>
      </div>
    </div>
  )
}
