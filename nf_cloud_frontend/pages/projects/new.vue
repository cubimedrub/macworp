<template>
    <div>
        <h1>Start a new project</h1>
        <div class="row mb-3">
            <label for="project-name" class="col-sm-2 col-form-label">Name</label>
            <div class="col-sm-10 d-flex flex-column justify-content-center">
                <input v-model="name" v-on:keyup.enter="createProject" id="project-name" class="form-control" type="text">
                <small v-if="errors.name">
                    <AttributeErrorList :errors="errors.name"></AttributeErrorList>
                </small>    
            </div>
        </div>
        <div class="d-flex justify-content-end">
            <button @click="createProject" :disable="is_creating" type="button" class="btn btn-primary">
                submit
                <i class="fas fa-angle-right"></i>
            </button>
        </div>
    </div>
</template>


<script>
export default {
    data(){
        return {
            name: null,
            is_creating: false,
            errors: {}
        }
    },
    activated(){
        this.name = null
        this.is_creating = false
    },
    methods: {
        createProject(){
            if(!this.is_creating){
                this.is_creating = true
                return fetch(`${this.$config.nf_cloud_backend_base_url}/api/projects/create`, {
                    method: "POST",
                    cache: "no-cache",
                    headers: {
                        "Content-Type": "application/json",
                        "x-access-token": this.$store.state.login.jwt
                    },
                    body: JSON.stringify({
                        name: this.name
                    })
                }).then(response => {
                    if(response.ok) {
                        response.json().then(response_data => {
                            this.name = null
                            this.$router.push({name: "projects-id", params: {id: response_data.id}})
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
