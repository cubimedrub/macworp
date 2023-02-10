<template>
    <main v-if="$store.state.login.jwt != null" class="container-fluid mr-0">
        <div class="row application-content-row">
            <nav :class="{'extended-menu': show_menu}" class="col-12 col-lg-2 position-sticky d-flex flex-column py-3 application-nav-column collapsed-menu">
                <div class="d-flex justify-content-between align-items-center">
                    <h1 class="brand text-break">
                        <NuxtLink to="/">
                            Nextflow Cloud
                        </NuxtLink>
                    </h1>
                    <button @click="toggleMenu" type="button" class="btn btn-sm btn-outline-primary d-lg-none">
                        <i :class="{'fa-times': show_menu, 'fa-bars': !show_menu}" class="fas"></i>
                    </button>
                </div>
                <nav class="d-flex flex-column flex-fill application-menu ">
                    <div class="flex-fill">
                        <p class="application-menu-label">
                                Explore the database
                        </p>
                        <ul class="nav flex-column">
                                <li class="nav-item">
                                        <NuxtLink to="/projects" class="btn btn-outline-primary nav-link mb-1">
                                                <i class="fas fa-random"></i>
                                                <span>Project</span>
                                        </NuxtLink>
                                </li>
                            <li class="nav-item">
                                <NuxtLink to="/workflows" class="btn btn-outline-primary nav-link">
                                    <i class="fas fa-project-diagram"></i>
                                    <span>Workflows</span>
                                </NuxtLink>
                            </li>
                        </ul>
                        <p class="application-menu-label">
                        </p>
                        <ul class="nav flex-column">
                            <li class="nav-item">
                                <NuxtLink to="/docs" class="btn btn-outline-primary nav-link mt-1">
                                    <i class="fas fa-file-code"></i>
                                    <span>Documentation</span>
                                </NuxtLink>
                            </li>
                        </ul>
                    </div>
                    <ul class="nav flex-column justify-content-end">
                        <li class="nav-item">
                            <NuxtLink to="/errors" class="nav-link">
                                <i class="fas fa-exclamation-circle"></i>
                                <span>Errors</span>
                                <span v-if="$store.state.errors.errors.length" class="badge badge-pill bg-danger">{{ $store.state.errors.errors.length }}</span>
                            </NuxtLink>
                        </li>
                        <li class="nav-item">
                            <button @click="logout" type="button" class="btn btn-link text-decoration-none">
                                <i class="fas fa-sign-out-alt"></i>
                                <span>Logout</span>
                            </button>
                        </li>
                    </ul>
                    <div class="external-pages-menu">
                        <ul class="nav flex-column">
                            <li><a href="https://www.ruhr-uni-bochum.de/de/impressum" target="_blank">Legal information</a></li>
                            <li><a href="https://www.ruhr-uni-bochum.de/de/datenschutz" target="_blank">Privacy</a></li>
                        </ul>
                        <div class="sponsors-logos">
                            <a href="https://www.ruhr-uni-bochum.de" target="_blank"><img class="w-25" src="~/assets/images/rub-logo-small.png" alt="Logo of the Ruhr University Bochum"/></a>
                            <a href="https://www.ruhr-uni-bochum.de/mpc/" target="_blank"><img class="w-25" src="~/assets/images/mpc-logo-small.png" alt="Logo of the Medical Proteom Center"/></a>
                        </div>
                    </div>
                </nav>
            </nav>
            <div class="col-12 col-lg-10">
                <div class="container-fluid py-3">
                    <div class="content">
                        <Nuxt keep-alive />
                    </div>
                </div>
            </div>
        </div>
    </main>
</template>

<script>

export default {
    middleware: [
        "validate_login"
    ],
    data(){
        return {
            show_menu: false
        }
    },
    methods: {
        toggleMenu(){
            this.show_menu = !this.show_menu
        },
        async logout(){
            return fetch(
                `${this.$config.nf_cloud_backend_base_url}/api/users/logout`,
                {
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    },
                }
            ).then(response => {
                if(response.ok) {
                    this.$store.commit("login/invalidate")
                    this.$router.push({name: "login"})
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        }
    }
}
</script>

<style>
</style>
