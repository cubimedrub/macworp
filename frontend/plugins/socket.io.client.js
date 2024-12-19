import io from 'socket.io-client'

export default ({ app }, inject) => {
    inject(
        'socket',
        io(
            app.$config.macworp_ws_url, {
                transports: ["websocket", "polling"]
            }
        )
    )
}
