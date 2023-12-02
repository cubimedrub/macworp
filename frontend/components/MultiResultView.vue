<template>
    <!-- for now only to use with Plotly plots -->
    <div>
        <h2>{{ header }}</h2> 
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
                    <li v-for="path in plotly_jsons" @click="selectJson(path)">
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
            selected_json: null
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
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        // init the dropdown
        this.initDropdown(this.$refs["paths-dropdown"])
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
    }
}
</script>