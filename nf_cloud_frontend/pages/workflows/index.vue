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
                        <div class="d-flex justify-content-between">
                            <template v-if="workflow.is_published">
                                <i class="fas fa-circle small" style="color: green; margin-top: 0.8em"></i>
                            </template>
                            <template v-else>
                                <i class="fas fa-circle small" style="color: #dc3545; margin-top: 0.8em"></i>
                            </template>
                            <h5 class="card-title" style="margin-top: 0.3em">{{ workflow.name }}</h5>
                            <NuxtLink :to="{name: 'workflows-id', params: {'id': workflow.id}}" class="btn btn-outline-primary btn-sm" style="margin-bottom: 0.1em">
                                <i class="fas fa-edit"></i>
                                Edit
                            </NuxtLink>
                        </div>
                        <p class="card-text">{{workflow.description}}</p>
                    </div>
                </div>
            </div>
        </div>
    </div>
</template>

<script>

import Vue from "vue";

const RELOAD_WORKFLOW_FILES_EVENT = "RELOAD_WORKFLOW_FILES"
const DELETE_CONFIRMATION_DIALOG_ID = "delete_confirmation_dialog"
export default {
    data(){
        return {
            workflows: [],
            local_event_bus: new Vue(),
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
        },
        deleteWorkflow(workflow_id){
            return fetch(
                `${this.$config.nf_cloud_backend_base_url}/api/workflows/${workflow_id}/delete`,
                {
                    method: "POST",
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    }
                }
            ).then(response => {
                if(response.ok || response.status == 404) {
                    this.$router.push({name: "workflows"})
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        /**
         * Opens the delete confirmation dialog
         */
        showDeleteDialog(){
            this.local_event_bus.$emit("CONFIRMATION_DIALOG_OPEN", this.delete_confirmation_dialog_id)
        },
    },
    computed: {
        /**
         * Returns the argument change event so it is usable in the template.
         *
         * @returns {string}
         */
        argument_changed_event(){
            return ARGUMENT_CHANGED_EVENT
        },
        /**
         * Provide access to RELOAD_WORKFLOW_FILES_EVENT in vue instance.
         * @return {string}
         */
        reload_project_files_event(){
            return RELOAD_WORKFLOW_FILES_EVENT
        },
        /**
         * Provide access to DELETE_CONFIRMATION_DIALOG_ID in vue instance.
         * @return {string}
         */
        delete_confirmation_dialog_id() {
            return DELETE_CONFIRMATION_DIALOG_ID
        },
    }
}
</script>

<style>
.card-body{
    padding: 0.5em;
}
.card-text{
    width: 100%;
    height: 120px;
    padding: 7px;
    font-size: 15px;
    border: 1px solid #e7e7e7;
    border-radius: 0.25rem;
    margin-top: 0.5em;
    overflow-y: scroll;
    -ms-overflow-style: none; /* Internet Explorer 10+ */
    scrollbar-width: none; /* Firefox */
}

.card-text::-webkit-scrollbar {
    width: 0;
    background-color: transparent; /* Safari and Chrome */
}
</style>
