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
                        <p class="card-text">Hier können wir vielleicht beschreibung einfügen</p>
                        <div class="d-flex justify-content-between">
                            <NuxtLink :to="{name: 'workflows-id', params: {'id': workflow.id}}" class="btn btn-outline-primary btn-sm">
                                <i class="fas fa-edit"></i>
                                Edit
                            </NuxtLink>
                            <button @click="showDeleteDialog" type="button" class="btn btn-outline-danger btn-sm">
                                <i class="fas fa-trash-alt"></i>
                                Delete
                            </button>
                        </div>
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
</style>
