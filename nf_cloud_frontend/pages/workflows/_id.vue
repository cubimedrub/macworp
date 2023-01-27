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
            <tr>
                <th>Description</th>
                <td><textarea v-model="workflow.description"></textarea></td>
            </tr>
            <tr>
                <th>Publish</th>
                <td><input type="checkbox" v-model="workflow.is_published"></td>
            </tr>
            </tbody>
        </table>
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
                        description: this.workflow.description,
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
