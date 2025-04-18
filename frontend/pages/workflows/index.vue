<template>
    <div>
        <div class="d-flex justify-content-between align-items-center">
            <h1>Workflows Dashboard</h1>
            <NuxtLink to="/workflows/new" class="btn btn-primary">
                <i class="fas fa-plus"></i>
                Create new workflow
            </NuxtLink>
        </div>
        <ul class="list-group">
            <NuxtLink v-for="workflow in workflows" :to="{name: 'workflows-id', params: {'id': workflow.id}}" :key="workflow.id" class="list-group-item list-group-item-action d-flex justify-content-between align-items-start">
                <div class="ms-2 me-auto">
                    <div class="fw-bold">{{ workflow.name }}</div>
                    {{workflow.description}}
                </div>
                <span v-if="workflow.is_published" class="badge text-bg-primary rounded-pill">published</span>
                <span v-else class="badge text-bg-warning rounded-pill">not published</span>
            </NuxtLink>
        </ul>
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
                `${this.$config.macworp_base_url}/api/workflows`, {
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
                `${this.$config.macworp_base_url}/api/workflows/${workflow_id}/delete`,
                {
                    method: "POST",
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    }
                }
            ).then(response => {
                if(response.ok || response.status == 404) {
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
