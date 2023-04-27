export const state = () => ({
    initialized: false,
    jwt: process.client ? localStorage.getItem("token") : null
})

export const mutations = {
    setJwt(state, token) {
        state.jwt = token
        localStorage.setItem("token", token)
    },
    initialize(state){
        if(!state.initialized){
            state.initialized = true
            state.jwt = localStorage.getItem("token")
        }
    },
    invalidate(state){
        localStorage.removeItem("token")
        state.jwt = null
    }
}