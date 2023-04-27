/**
 * Checks if the given JWT token is valid and the user is still logged in.
 * If the token is refreshed, it will be stored in the store
 * 
 * @param {*} config NuxtJS config
 * @param {*} store NuxtJS store
 * @returns Promise.reject() or Promise.resolve(bool)
 */
export default async function validateLogin(config, store){
    return fetch(
        `${config.nf_cloud_backend_base_url}/api/users/logged-in`,
        {
            headers: {
                "x-access-token": store.state.login.jwt
            }
        }
    ).then(response => {
        if(response.ok) return Promise.resolve(true)
        else if (response.status == 401) {
            // invalidate login
            store.commit("login/invalidate")
            return Promise.resolve(false)
        } else if (response.status == 302) {
            if(response.headers.has("Location")){
                // parse url in location header params to get the new token
                const params = new Proxy(new URLSearchParams(response.headers.get("Location")), {
                    get: (searchParams, prop) => searchParams.get(prop),
                })
                store.commit("login/setJwt", params.token)
                return Promise.resolve(true)
            }
            return Promise.reject("'Location'-header not found. Can not get token.")
        }
        return Promise.reject("Response status was unknown.")
    })
}