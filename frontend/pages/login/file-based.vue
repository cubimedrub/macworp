<template>
    <div class="row justify-content-center login ">
        <div class="card" style="width: 18rem;">
            <div class="card-body">
                <div v-if="'general' in errors" class="alert alert-danger" role="alert">
                    {{ errors.general }}
                </div>
                <label for="login_id" class="form-label">Login ID</label>
                <input v-on:keyup.enter="login" v-model="login_id" type="text" class="form-control" id="login_id">
                <label for="password" class="form-label">Password</label>
                <input v-on:keyup.enter="login" v-model="password" type="password" class="form-control" id="password">
                <div class="d-flex justify-content-end mt-2">
                    <button @click.prevent="login" class="btn btn-primary">Login</button>
                </div>
            </div>
        </div>
    </div>
</template>

<script>
export default {
    layout: "login",
    data(){
        return {
            login_id: null,
            password: null,
            callback_url: null,
            is_logging_in: false,
            errors: {},
        }
    },
    mounted(){
        this.callback_url = this.$route.query.callback
    },
    methods: {
        async login(){
            if(!this.is_logging_in){
                this.is_logging_in = true
                this.errors = {}
                return fetch(this.callback_url, {
                    method: "POST",
                    headers: {
                        "Content-Type": "application/json",
                    },
                    body: JSON.stringify({
                        login_id: this.login_id,
                        password: this.password,
                    })
                }).then(response => {
                    if(response.status == 200){
                        response.json().then(data => {
                            // redirect to callback url with token to save it in local storage
                            this.$router.push({
                                name: "login-callback",
                                query: {token: data.token}
                            })
                        })
                    } else {
                        response.json().then(data => {
                            this.errors = data.errors
                        })
                    }
                })
                .finally(() => {
                    this.is_logging_in = false
                })
            }
        },
    }
}
</script>

<style>

</style>