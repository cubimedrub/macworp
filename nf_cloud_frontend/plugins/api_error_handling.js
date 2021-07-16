import Vue from 'vue'

Vue.mixin({
    methods: {
        handleUnknownResponse(response){
            var new_error = {
                "title": `API responded with ${response.status} - ${response.statusText}`,
                "date": new Date().toISOString().toString(),
                "description": response.statusText
            }
            if(response.headers.has("Date")){
                new_error.date = new Date(response.headers.get("Date")).toUTCString()
            }
            response.clone().json().then(response_data => {
                if(response_data.hasOwnProperty('errors') && response_data.errors.hasOwnProperty('general')){
                    new_error.description = response_data.errors.general
                } else {
                    new_error.description = JSON.stringify(response_data)
                }
            }).catch(() => {
                response.text().than(response_data => {
                    new_error.description = response_data
                }).catch(() => {
                    new_error.description = "Can not parse response."
                })
            }).finally(() => {
                this.$store.commit("errors/add", new_error)
                this.$router.push({ name: 'errors', query: { is_new_error: true } })
            })
        },
    }
})