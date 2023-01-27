<template>
    <div>
        <div class="d-flex justify-content-between align-items-center">
            <h1>Workflows</h1>
            <NuxtLink to="/workflows/new" class="btn btn-primary">
                <i class="fas fa-plus"></i>
                Create new workflow
            </NuxtLink>
        </div>
        <div class="row">
            <div v-for="workflow in workflows" :key="workflow.id" class="col-sm-3 mb-3">
                <div class="card shadow-sm">
                    <div class="card-body">
                        <h5 class="card-title">{{ workflow.name }}</h5>
                        <p class="card-text">{{workflow.description}}</p>
                        <div class="d-flex justify-content-between">
                            <NuxtLink :to="{name: 'workflows-id', params: {'id': workflow.id}}" class="btn btn-outline-primary btn-sm">
                                <i class="fas fa-edit"></i>
                                Edit
                            </NuxtLink>
                        </div>
                    </div>
                </div>
            </div>
        </div>
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
