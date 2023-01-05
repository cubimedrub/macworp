<template>
    <div v-if="workflow != null">
        <div class="d-flex justify-content-between align-items-center">
            <h1>Workflow "{{ workflow.name }}"</h1>
            <button @click="showDeleteDialog" type="button" class="btn btn-danger">
                <i class="fas fa-trash-alt"></i>
                Delete
            </button>
        </div>
        <table class="table">
            <tbody>
            <tr>
                <th>ID</th>
                <td>{{  workflow.id }}</td>
            </tr>
            <tr>
                <th>Name</th>
                <td>{{  workflow.name }}</td>
            </tr>
            </tbody>
        </table>
        <div class="d-flex justify-content-between">
            <div class="d-flex justify-content-start" style="margin-bottom: 0.5em">
                <button @click="" type="button" class="btn btn-primary" style="margin-right: 0.5em">
                    <i class="fas fa-file-import me-2"></i>
                    Import
                </button>
                <button @click="" type="button" class="btn btn-primary" style="margin-right: 0.5em">
                    <i class="fas fa-file-export me-2"></i>
                    Export
                </button>
            </div>
            <div class="d-flex justify-content-end" style="margin-bottom: 0.5em">
                <button @click="updateWorkflow" type="button" class="btn btn-primary" style="margin-right: 0.5em">
                    <i class="fas fa-save me-2"></i>
                    Save
                </button>
                <NuxtLink to="/workflows">
                    <button type="button" class="btn btn-outline-primary">
                        <i class="fas fa-project-diagram me-2"></i>
                        Go Back
                    </button>
                </NuxtLink>
            </div>
        </div>

        <tiptap-editor v-model="workflow.definition"/>
        <ConfirmationDialog :local_event_bus="local_event_bus" :on_confirm_func="deleteWorkflow" :identifier="delete_confirmation_dialog_id" confirm_button_class="btn-danger">
            <template v-slot:header>
                Delete this Workflow?
            </template>
            <template v-slot:body>
                Are you sure you want to delete this Workflow?
            </template>
            <template v-slot:dismiss-button>
                Dismiss
            </template>
            <template v-slot:confirm-button>
                Delete
            </template>
        </ConfirmationDialog>

    </div>
</template>

<script>
import Vue from "vue";
import TiptapEditor from '~/components/TiptapEditor.vue'
const RELOAD_WORKFLOW_FILES_EVENT = "RELOAD_WORKFLOW_FILES"
const DELETE_CONFIRMATION_DIALOG_ID = "delete_confirmation_dialog"
const START_WORKFLOW_CONFIRMATION_DIALOG_ID = "start_workflow_confirmation_dialog"

function isJsonString(str) {
    try {
        JSON.parse(str);
    } catch (e) {
        return false;
    }
    return true;
}
export default {
    data(){
        return {
            workflow: null,
            is_updating: false,
            errors: {},
            local_event_bus: new Vue(),
        }
    },
    activated(){
        this.fetchWorkflow()
    },
    components: {
        TiptapEditor
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
        deleteWorkflow(){
            return fetch(
                `${this.$config.nf_cloud_backend_base_url}/api/workflows/${this.$route.params.id}/delete`,
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
                        definition: this.workflow.definition,
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
        /**
         * Provide access to START_WORKFLOW_CONFIRMATION_DIALOG_ID in vue instance.
         * @return {string}
         */
        start_workflow_confirmation_dialog_id() {
            return START_WORKFLOW_CONFIRMATION_DIALOG_ID
        }
    }
}
</script>

<style>
.ProseMirror{
    border-radius: 0.5rem;
    font-size: 1.5rem;
    border: 1px solid black;
}
</style>
