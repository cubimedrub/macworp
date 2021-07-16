export const state = () => ({
    errors: []
})

export const mutations = {
    add(state, new_error) {
        state.errors.push(new_error)
    },
    remove(state, idx){
        state.errors.splice(idx, 1)
    }
}