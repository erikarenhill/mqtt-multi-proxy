import { useEffect, useState, useCallback } from 'react'
import './MessageViewer.css'

interface MqttMessage {
  timestamp: string
  client_id: string
  topic: string
  payload: number[]
  qos: number
  retain: boolean
}

interface TopicNode {
  name: string
  fullPath: string
  children: Map<string, TopicNode>
  messages: MqttMessage[]
  expanded: boolean
}

function MessageViewer() {
  const [messages, setMessages] = useState<MqttMessage[]>([])
  const [topicTree, setTopicTree] = useState<Map<string, TopicNode>>(new Map())
  const [selectedTopic, setSelectedTopic] = useState<string | null>(null)
  const [connected, setConnected] = useState(false)
  const [lastMessageTime, setLastMessageTime] = useState<string | null>(null)
  const [maxMessages] = useState(1000) // Keep last 1000 messages

  useEffect(() => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const wsUrl = `${protocol}//${window.location.host}/ws/messages`

    let ws: WebSocket
    let reconnectTimeout: ReturnType<typeof setTimeout>

    const connect = () => {
      ws = new WebSocket(wsUrl)

      ws.onopen = () => {
        console.log('WebSocket connected')
        setConnected(true)
      }

      ws.onmessage = (event) => {
        try {
          const msg: MqttMessage = JSON.parse(event.data)

          // Update last message timestamp (only if newer)
          setLastMessageTime(prev => {
            if (!prev) return msg.timestamp
            const newTime = new Date(msg.timestamp).getTime()
            const currentTime = new Date(prev).getTime()
            return newTime > currentTime ? msg.timestamp : prev
          })

          setMessages(prev => {
            const updated = [...prev, msg]
            // Keep only the last maxMessages
            return updated.slice(-maxMessages)
          })

          // Update topic tree
          setTopicTree(prev => {
            const newTree = new Map(prev)
            insertIntoTree(newTree, msg)
            return newTree
          })
        } catch (error) {
          console.error('Failed to parse message:', error)
        }
      }

      ws.onclose = () => {
        console.log('WebSocket disconnected')
        setConnected(false)
        // Reconnect after 3 seconds
        reconnectTimeout = setTimeout(connect, 3000)
      }

      ws.onerror = (error) => {
        console.error('WebSocket error:', error)
      }
    }

    connect()

    return () => {
      if (ws) ws.close()
      if (reconnectTimeout) clearTimeout(reconnectTimeout)
    }
  }, [maxMessages])

  const insertIntoTree = (tree: Map<string, TopicNode>, msg: MqttMessage) => {
    const parts = msg.topic.split('/')
    let currentLevel = tree

    parts.forEach((part, index) => {
      const fullPath = parts.slice(0, index + 1).join('/')

      if (!currentLevel.has(part)) {
        currentLevel.set(part, {
          name: part,
          fullPath,
          children: new Map(),
          messages: [],
          expanded: false,
        })
      }

      const node = currentLevel.get(part)!

      if (index === parts.length - 1) {
        // Leaf node - add message
        node.messages = [...node.messages, msg].slice(-10) // Keep last 10 messages per topic
      }

      currentLevel = node.children
    })
  }

  const toggleExpand = useCallback((fullPath: string) => {
    setTopicTree(prev => {
      const newTree = new Map(prev)
      toggleNodeExpanded(newTree, fullPath)
      return newTree
    })
  }, [])

  const toggleNodeExpanded = (tree: Map<string, TopicNode>, fullPath: string) => {
    const parts = fullPath.split('/')
    let currentLevel = tree

    parts.forEach((part, index) => {
      const node = currentLevel.get(part)
      if (node) {
        if (index === parts.length - 1) {
          node.expanded = !node.expanded
        }
        currentLevel = node.children
      }
    })
  }

  const renderTree = (nodes: Map<string, TopicNode>, level = 0): JSX.Element[] => {
    return Array.from(nodes.entries()).map(([, node]) => {
      const hasChildren = node.children.size > 0
      const hasMessages = node.messages.length > 0
      const isSelected = selectedTopic === node.fullPath

      return (
        <div key={node.fullPath} style={{ marginLeft: `${level * 12}px` }}>
          <div
            className={`topic-node ${isSelected ? 'selected' : ''} ${hasMessages ? 'has-messages' : ''}`}
            onClick={() => {
              if (hasMessages) {
                setSelectedTopic(node.fullPath)
              }
              if (hasChildren) {
                toggleExpand(node.fullPath)
              }
            }}
          >
            {hasChildren && (
              <span className="expand-icon">{node.expanded ? '▼' : '▶'}</span>
            )}
            <span className="topic-name">{node.name}</span>
            {hasMessages && (
              <span className="message-count">{node.messages.length}</span>
            )}
          </div>
          {node.expanded && hasChildren && renderTree(node.children, level + 1)}
        </div>
      )
    })
  }

  const getSelectedMessages = (): MqttMessage[] => {
    if (!selectedTopic) return []

    const parts = selectedTopic.split('/')
    let currentLevel = topicTree

    for (const part of parts) {
      const node = currentLevel.get(part)
      if (!node) return []
      if (parts[parts.length - 1] === part) {
        return node.messages
      }
      currentLevel = node.children
    }

    return []
  }

  const formatPayload = (payload: number[]): string => {
    try {
      const text = new TextDecoder().decode(new Uint8Array(payload))
      // Try to parse as JSON
      const json = JSON.parse(text)
      return JSON.stringify(json, null, 2)
    } catch {
      // Not JSON, return as text
      try {
        return new TextDecoder().decode(new Uint8Array(payload))
      } catch {
        // Binary data
        return `Binary data (${payload.length} bytes)`
      }
    }
  }

  const formatTimestamp = (timestamp: string): string => {
    return new Date(timestamp).toLocaleString()
  }

  return (
    <div className="message-viewer">
      <div className="viewer-header">
        <div>
          <h3>Live Messages</h3>
          {lastMessageTime && (
            <div className="last-update">
              Last: {formatTimestamp(lastMessageTime)}
            </div>
          )}
        </div>
        <div className={`connection-status ${connected ? 'connected' : 'disconnected'}`}>
          {connected ? '🟢 Connected' : '🔴 Disconnected'}
        </div>
      </div>

      <div className="viewer-content">
        <div className="topic-tree-panel">
          <div className="panel-header">
            <h4>Topics</h4>
            <span className="total-count">{messages.length} messages</span>
          </div>
          <div className="topic-tree">
            {topicTree.size === 0 ? (
              <div className="empty-state">
                <p>No messages yet</p>
                <p className="hint">Publish an MQTT message to see it here</p>
              </div>
            ) : (
              renderTree(topicTree)
            )}
          </div>
        </div>

        <div className="message-details-panel">
          <div className="panel-header">
            <h4>Message Details</h4>
            {selectedTopic && <span className="selected-topic">{selectedTopic}</span>}
          </div>
          <div className="message-list">
            {selectedTopic ? (
              getSelectedMessages().length > 0 ? (
                (() => {
                  const messages = getSelectedMessages()
                  const lastMessage = messages[messages.length - 1]
                  return (
                    <div className="message-card">
                      <div className="message-meta">
                        <span className="timestamp">{formatTimestamp(lastMessage.timestamp)}</span>
                        <span className="client-id">Client: {lastMessage.client_id}</span>
                        <span className={`qos qos-${lastMessage.qos}`}>QoS {lastMessage.qos}</span>
                        {lastMessage.retain && <span className="retain-badge">Retained</span>}
                      </div>
                      <div className="payload-container">
                        <div className="payload-header">Payload:</div>
                        <pre className="payload-content">{formatPayload(lastMessage.payload)}</pre>
                      </div>
                    </div>
                  )
                })()
              ) : (
                <div className="empty-state">No messages</div>
              )
            ) : (
              <div className="empty-state">
                <p>Select a topic to view messages</p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

export default MessageViewer
