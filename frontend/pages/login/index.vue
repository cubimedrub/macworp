<template>
    <div class="row justify-content-center login ">
        <template v-for="(providers, provider_type) in login_providers">
            <div v-for="(description, provider) in providers" :key="`${provider_type}_${provider}`" class="card" style="width: 18rem;">
                <div class="card-body">
                    <h5 class="card-title">{{provider | capitalize}}</h5>
                    <p class="card-text">{{ description}}</p>
                    <a :href="get_login_url(provider_type, provider)" class="btn btn-primary">
                        Login
                    </a>
                </div>
            </div>
        </template>
    </div>
</template>

<script>
export default {
    layout: "login",
    data(){
        return {
            login_providers: null
        }
    },
    mounted(){
        if(this.$store.state.login.jwt == null){
            this.get_login_provider()
        } else {
            this.$router.replace("/")
        }
    },
    methods: {
        async get_login_provider(){
            return fetch(`${this.$config.nf_cloud_backend_base_url}/api/users/login-providers`)
            .then(response => {
                return response.json().then(data => {
                    this.login_providers = data
                })
            })
        },
        get_login_url(provider_type, provider){
            return `${this.$config.nf_cloud_backend_base_url}/api/users/${provider_type}/${provider}/login`
        }
    },
    filters: {
        capitalize(value) {
            if (!value) return ''
            value = value.toString()
            return value.charAt(0).toUpperCase() + value.slice(1)
        }
    }
}
</script>

<style>

</style>