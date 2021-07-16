<template>
    <div>
        <div class="d-flex justify-content-between align-items-center">
            <h1>Workflows</h1>
            <NuxtLink to="/workflows/new" class="btn btn-primary btn-sm">
                <i class="fas fa-plus"></i>
                start new workflow
            </NuxtLink>
        </div>
        <ul v-if="workflows">
            <li v-for="workflow in workflows" :key="workflow.id">
                <NuxtLink :to="{name: 'workflows-id', params: {'id': workflow.id}}">
                    {{ workflow.name }}
                </NuxtLink>
            </li>
        </ul>
        <Pagination :parent_event_bus="local_event_bus" :total_items="total_workflow_count" :items_per_page="workflows_per_page"></Pagination>
    </div>
</template>

<script>
import Vue from "vue"

const WORKFLOWS_PER_PAGE = 50

export default {
    data(){
        return {
            local_event_bus: new Vue(),
            current_workflows_page: 1,
            workflows: null,
            total_workflow_count: 0,
        }
    },
    created(){
        this.local_event_bus.$on("PAGE_CHANGED", page => this.goToPage(page))
        this.loadTotalWorkflowCount().then(() => {
            this.loadWorkflows()
        })
    },
    methods: {
        async loadTotalWorkflowCount(){
            return fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows/count`)
            .then(response => {
                if(response.ok) {
                    response.json().then(response_data => {
                        this.total_workflow_count = response_data.count
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        async loadWorkflows(){
            var query_string = `?offset=${this.offset}&limit=${this.workflows_per_page}`
            return fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows${query_string}`, {
                cache: 'no-cache',
            })
            .then(response => {
                if(response.ok) {
                    response.json().then(response_data => {
                        this.workflows = response_data.workflows
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        goToPage(page){
            this.current_workflows_page = page
            this.loadWorkflows()
        }
    },
    computed: {
        workflows_per_page(){
            return WORKFLOWS_PER_PAGE
        },
        offset(){
            return (this.current_workflows_page  - 1) * this.workflows_per_page
        }
    }
}
</script>
