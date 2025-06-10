import validateLogin from "../utils/validate_login"

/**
 * Middleware for default layout which checks if the login is still valid.
 */
export default async function ({store, redirect, $config, route}) {
    if(process.client){
        if (route.path.startsWith("/ping")) return Promise.resolve(true)
        store.commit("login/initialize")
        if(store.state.login.jwt == null) return redirect('/login')
        validateLogin($config, store)
            .then(is_valid => {
                if(!is_valid) return redirect('/login')
            })
            .catch(error => {
                console.error(error)
                return redirect('/login')
            })
    }
}