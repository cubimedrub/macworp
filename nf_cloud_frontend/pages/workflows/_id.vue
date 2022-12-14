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

    </div>
</template>

<script>
import Vue from "vue";
import TiptapEditor from '~/components/TiptapEditor.vue'

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
