import Vue from "vue"


/**
 * Mixin for components that will render results.
 */
export default {
    props: {
        project_id: {
            type: Number,
            required: true
        }
    },
    methods: {
        async authenticateFileDownload(path) {
            return fetch(`${this.$config.nf_cloud_backend_base_url}/api/users/one-time-use-token`, {
                headers: {
                    "x-access-token": this.$store.state.login.jwt
                }
            }).then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        return `${this.$config.nf_cloud_backend_base_url}/api/projects/${this.project_id}/download?path=${path}&one-time-use-token=${response_data.token}`
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        }
    }   
}