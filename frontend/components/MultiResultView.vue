<template>
    <!-- for now only to use with Plotly plots -->
    <div>
        <h2>{{ header }}</h2> 
        <div v-if="!loading_file_list && plotly_jsons.length > 0">
            <div class="d-flex justfy-content-between">
                <div class="dropdown">
                    <button class="btn btn-secondary dropdown-toggle" type="button" data-bs-toggle="dropdown" aria-expanded="false" ref="paths-dropdown">
                        <template v-if="selected_json">
                            {{ selected_json }}
                        </template>
                        <template v-else>
                            Select a plot
                        </template>
                    </button>
                    <ul class="dropdown-menu">
                        <li v-for="path in plotly_jsons" :key="path" @click="selectJson(path)">
                            <div class="dropdown-item">{{ path }}</div>
                        </li>
                    </ul>
                </div>
            </div>
            <div>
                <div v-if="selected_json">
                    <PlotlyViewer
                        :project_id="project_id"
                        :header="''"
                        :description="''"
                        :path="`${path}/${selected_json}`"
                    ></PlotlyViewer>
                </div> 
            </div>
            <p>{{ description }}</p>
        </div>
        <div v-if="!loading_file_list && plotly_jsons.length == 0">
            <p>
                Files not ready yet.
            </p>
        </div>
    </div>
</template>


<script>

export default {
    props: {
        project_id: {
            type: Number,
            required: true
        },
        header: {
            type: String,
            required: true
        },
        path: {
            type: String,
            required: true
        },
        description: {
            type: String,
            required: true
        }
    },
    data(){
        return {
            plotly_jsons: [],
            selected_json: null,
            loading_file_list: true
        }
    },
    mounted() {
        // fetch all json files in the directory
        fetch(`${this.$config.nf_cloud_backend_base_url}/api/projects/${this.project_id}/files?dir=${this.path}`, {
            headers: {
                "x-access-token": this.$store.state.login.jwt
            }
        }).then(response => {
            if(response.ok) {
                return response.json().then(data => {
                    this.plotly_jsons = data.files.filter(path => path.endsWith(".json"))
                    if(this.plotly_jsons.length > 0) {
                        this.selectJson(this.plotly_jsons[0])
                    }
                })
            } else if (response.status == 404) {
                return Promise.resolve(null)
            } else {
                return this.handleUnknownResponse(response)
            }
        }).finally(() => {
            this.loading_file_list = false
        })
    },
    methods: {
        /**
         * Forces the plotly view to rerender
         * 
         * @param {*} json 
         */
        selectJson(json){
            this.selected_json = null
            this.$nextTick(() => {
                this.selected_json = json
            })
        }
    },
    watch: {
        /**
         * If the selected json changes, we need to rerender the plotly view
         */
        loading_file_list(new_val){
            if (new_val && this.plotly_jsons.length > 0) {
                this.$nextTick(() => {
                    // init the dropdown
                    this.initDropdown(this.$refs["paths-dropdown"])
                })
            }
        }
    }
}
</script>