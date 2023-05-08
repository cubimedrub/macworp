<template>
    <div>
        <h1>Start a new workflow</h1>
        <div class="row mb-3">
            <label for="workflow-name" class="col-sm-2 col-form-label">Name</label>
            <div class="col-sm-10 d-flex flex-column justify-content-center">
                <input v-model="name" v-on:keyup.enter="createWorkflow" id="workflow-name" class="form-control" type="text">
                <small v-if="errors.name">
                    <AttributeErrorList :errors="errors.name" class="alert-danger" style="list-style: none"></AttributeErrorList>
                </small>
            </div>
        </div>
        <div class="row mb-3">
            <label for="workflow-description" class="col-sm-2 col-form-label">Description</label>
            <div class="col-sm-10 d-flex flex-column justify-content-center">
                <input v-model="description" v-on:keyup.enter="createWorkflow" id="workflow-description" class="form-control" type="text">
                <small v-if="errors.description">
                    <AttributeErrorList :errors="errors.description" class="alert-danger" style="list-style: none"></AttributeErrorList>
                </small>
            </div>
        </div>
        <div class="row mb-3">
            <div class="d-flex justify-content-end">
                <button @click="createWorkflow" :disable="is_creating" type="button" class="btn btn-primary">
                    Create
                    <i class="fas fa-angle-right"></i>
                </button>
            </div>
        </div>
    </div>
</template>


<script>
export default {
    data(){
        return {
            name: null,
            description: null,
            is_creating: false,
            errors: {}
        }
    },
    activated(){
        this.name = null
        this.description = null
        this.is_creating = false
    },
    methods: {
        createWorkflow(){
            if(!this.is_creating){
                this.is_creating = true
                return fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows/create`, {
                    method: "POST",
                    cache: "no-cache",
                    headers: {
                        "Content-Type": "application/json",
                        "x-access-token": this.$store.state.login.jwt
                    },
                    body: JSON.stringify({
                        name: this.name,
                        description: this.description
                    })
                }).then(response => {
                    if(response.ok) {
                        response.json().then(response_data => {
                            this.name = null
                            this.description = null
                            this.errors = {}
                            this.$router.push({name: "workflows-id", params: {id: response_data.id}})
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
                        this.is_creating = false
                    })
            }
        }
    }
}
</script>
