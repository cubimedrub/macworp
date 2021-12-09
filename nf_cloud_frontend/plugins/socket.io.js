import io from 'socket.io-client'

const socket = io(
    process.env.NF_CLOUD_BACKEND_BASE_URL || 'http://localhost:3001',
)

export default socket