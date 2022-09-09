<template>
    <div>
        <div class="d-flex justify-content-between align-items-center">
            <h1>Projects</h1>
            <NuxtLink to="/projects/new" class="btn btn-primary btn-sm">
                <i class="fas fa-plus"></i>
                start new project
            </NuxtLink>
        </div>
        <ul v-if="projects">
            <li v-for="project in projects" :key="project.id">
                <NuxtLink :to="{name: 'projects-id', params: {'id': project.id}}">
                    {{ project.name }}
                </NuxtLink>
            </li>
        </ul>
        <Pagination :parent_event_bus="local_event_bus" :total_items="total_project_count" :items_per_page="projects_per_page"></Pagination>
    </div>
</template>

<script>
import Vue from "vue"

const WORKFLOWS_PER_PAGE = 50

export default {
    data(){
        return {
            local_event_bus: new Vue(),
            current_projects_page: 1,
            projects: null,
            total_project_count: 0,
        }
    },
    created(){
        this.local_event_bus.$on("PAGE_CHANGED", page => this.goToPage(page))
        this.loadTotalProjectCount().then(() => {
            this.loadProjects()
        })
    },
    methods: {
        async loadTotalProjectCount(){
            return fetch(
                `${this.$config.nf_cloud_backend_base_url}/api/projects/count`,
                {
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    },
                }
            ).then(response => {
                if(response.ok) {
                    response.json().then(response_data => {
                        this.total_project_count = response_data.count
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        async loadProjects(){
            var query_string = `?offset=${this.offset}&limit=${this.projects_per_page}`
            return fetch(
                `${this.$config.nf_cloud_backend_base_url}/api/projects${query_string}`, {
                headers: {
                    "x-access-token": this.$store.state.login.jwt
                },
                cache: "no-cache",
            })
            .then(response => {
                if(response.ok) {
                    response.json().then(response_data => {
                        this.projects = response_data.projects
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        goToPage(page){
            this.current_projects_page = page
            this.loadProjects()
        }
    },
    computed: {
        projects_per_page(){
            return WORKFLOWS_PER_PAGE
        },
        offset(){
            return (this.current_projects_page  - 1) * this.projects_per_page
        }
    }
}
</script>
