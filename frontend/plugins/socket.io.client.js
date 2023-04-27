import io from 'socket.io-client'

export default ({ app }, inject) => {
    inject(
        'socket',
        io(
            app.$config.nf_cloud_backend_ws_url, {
                transports: ["websocket", "polling"]
            }
        )
    )
}
