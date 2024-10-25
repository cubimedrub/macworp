<template>
    <div v-if="workflow != null">
        <div class="d-flex justify-content-between align-items-center">
            <h1>Workflow "{{ workflow.name }}"</h1>
            <div>
                <button @click="updateWorkflow" type="button" class="btn btn-primary">
                    <i class="fas fa-save me-2"></i>
                    Save
                </button>
                <button @click="showDeleteDialog" type="button" class="btn btn-danger">
                    <i class="fas fa-trash-alt"></i>
                    Delete
                </button>
                <NuxtLink to="/workflows">
                    <button type="button" class="btn btn-outline-primary">
                        <i class="fas fa-project-diagram"></i>
                        Go Back
                    </button>
                </NuxtLink>
            </div>
        </div>
        <table class="table mb-0">
            <tbody>
            <tr>
                <th>ID</th>
                <td>{{  workflow.id }}</td>
            </tr>
            <tr>
                <th>Name</th>
                <td>{{  workflow.name }}</td>
            </tr>
            <tr>
                <th>Description</th>
                <td>
                    <v-ace-editor
                        v-model="workflow.description"
                        @init="initDescriptionEditor"
                        theme="solarized_dark"
                        lang="markdown"
                        :options="{minHeight: description_editor_min_height, maxLines: Infinity, autoScrollEditorIntoView: true}"
                    />
                </td>
            </tr>
            <tr>
                <th>
                    Publish
                    <template v-if="workflow.is_published">
                        <i class="fas fa-circle small" style="color: green; margin-top: 0.8em"></i>
                    </template>
                    <template v-else>
                        <i class="fas fa-circle small" style="color: #dc3545; margin-top: 0.8em"></i>
                    </template>
                </th>
                <td>
                    <div class="form-check form-switch">
                        <input class="form-check-input" type="checkbox" id="flexSwitchCheckDefault" v-model="workflow.is_published">
                    </div>
                </td>
            </tr>
            </tbody>
        </table>
        <div class="row mb-3">
            <v-ace-editor
                v-model="workflow.definition"
                @init="initDefinitionEditor"
                theme="solarized_dark"
                lang="json"
                :options="{minHeight: definition_editor_min_height, maxLines: Infinity, autoScrollEditorIntoView: true}"
            />
            <small v-if="errors.definition">
                <AttributeErrorList :errors="errors.definition" class="alert-danger" style="list-style: none"></AttributeErrorList>
            </small>
        </div>
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
import toastr from "toastr";

const RELOAD_WORKFLOW_FILES_EVENT = "RELOAD_WORKFLOW_FILES"
const DELETE_CONFIRMATION_DIALOG_ID = "delete_confirmation_dialog"

const DEFINITION_EDITOR_MIN_HEIGHT = "200"
const DESCRIPTION_EDITOR_MIN_HEIGHT = "5"

export default {
    data(){
        return {
            workflow: null,
            is_updating: false,
            errors: {},
            local_event_bus: new Vue()
        }
    },
    activated(){
        this.fetchWorkflow()
    },
    methods: {
        initDescriptionEditor(editor){
            require('brace/ext/language_tools') //language extension prerequsite...
            require('brace/mode/markdown')
            require('brace/theme/solarized_dark')
            editor.setShowPrintMargin(false);
        },
        initDefinitionEditor(editor){
            require('brace/ext/language_tools') //language extension prerequsite...
            require('brace/mode/json')
            require('brace/theme/solarized_dark')
            editor.setShowPrintMargin(false);
        },
        fetchWorkflow(){
            return fetch(
                `${this.$config.macworp_base_url}/api/workflows/${this.$route.params.id}?definition_as_text=1`, {
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
                `${this.$config.macworp_base_url}/api/workflows/${this.$route.params.id}/delete`,
                {
                    method: "POST",
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    }
                }
            ).then(response => {
                if(response.ok) {
                    this.$router.push({name: "workflows"})
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        updateWorkflow(){
            if(!this.is_updating){
                this.is_updating = true
                this.errors = {}
                return fetch(`${this.$config.macworp_base_url}/api/workflows/${this.$route.params.id}/update`, {
                    method: "POST",
                    cache: "no-cache",
                    headers: {
                        "Content-Type": "application/json",
                        "x-access-token": this.$store.state.login.jwt
                    },
                    body: JSON.stringify({
                        definition: this.workflow.definition,
                        description: this.workflow.description,
                        is_published: this.workflow.is_published,
                    })
                }).then(response => {
                    if(response.ok) {
                        toastr.success(
                            "Successfully saved workflow",
                            "Saved"
                        )
                    } else if (response.status == 422) {
                        response.json().then(response_data => {
                            this.errors = response_data.errors
                        })
                        toastr.error(
                            "Errors occurred",
                            "error"
                        )
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
         * Provide access to DEFINITION_EDITOR_MIN_HEIGHT in vue instance.
         * @return {Number}
         */
        definition_editor_min_height() {
            return DEFINITION_EDITOR_MIN_HEIGHT
        },
        /**
         * Provide access to DESCRIPTION_EDITOR_MIN_HEIGHT in vue instance.
         * @return {Number}
         */
         description_editor_min_height() {
            return DESCRIPTION_EDITOR_MIN_HEIGHT
        }
    }
}
</script>
