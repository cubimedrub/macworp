<template>
    <div v-if="workflow != null">
        <h1>Workflow "{{ workflow.name }}"</h1>
        <textarea v-model="workflow.definition">
        </textarea>
        <button @click="updateWorkflow" type="button" class="btn btn-primary">
            <i class="fas fa-floppy mr-2"></i>
            save
        </button>
    </div>
</template>

<script>
export default {
    data(){
        return {
            workflow: null,
            is_updating: false,
            errors: {}
        }
    },
    activated(){
        this.fetchWorkflow()
    },
    methods: {
        fetchWorkflow(){
            return fetch(
                `${this.$config.nf_cloud_backend_base_url}/api/workflows/${this.$route.params.id}`, {
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    },
                    cache: "no-cache"
                }).then(response => {
                if(response.ok) {
                    response.json().then(response_data => {
                        this.workflow = response_data
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        updateWorkflow(){
            if(!this.is_updating){
                this.is_updating = true
                return fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows/${this.$route.params.id}/update`, {
                    method: "POST",
                    cache: "no-cache",
                    headers: {
                        "Content-Type": "application/json",
                        "x-access-token": this.$store.state.login.jwt
                    },
                    body: JSON.stringify({
                        definition: this.workflow.definition
                    })
                }).then(response => {
                    if(response.ok) {
                        response.json().then(response_data => {
                            this.errors = {}
                        })
                    } else if (response.status == 422) {
                        response.json().then(response_data => {
                            this.errors = response_data.errors
                        })
                    } else {
                        this.handleUnknownResponse(response)
                    }
                })
                    .finally(() => {
                        this.is_updating = false
                    })
            }
        }
    }
}
</script>

<style>
</style>
