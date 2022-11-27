<template>
    <div>
        <div class="d-flex justify-content-between align-items-center">
            <h1>Workflows</h1>
            <NuxtLink to="/workflows/new" class="btn btn-primary btn-sm">
                <i class="fas fa-plus"></i>
                create new workflow
            </NuxtLink>
        </div>
        <ul>
            <li v-for="workflow in workflows" :key="workflow.id">
                {{ workflow.name }}
            </li>
        </ul>
    </div>
</template>

<script>
export default {
    data(){
        return {
            workflows: []
        }
    },
    activated(){
        this.fetchWorkflows()
    },
    methods: {
        fetchWorkflows(){
            return fetch(
                `${this.$config.nf_cloud_backend_base_url}/api/workflows`, {
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    },
                    cache: "no-cache"
                }).then(response => {
                if(response.ok) {
                    response.json().then(response_data => {
                        this.workflows = response_data.workflows
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        }
    }
}
</script>

<style>
</style>
