<template>
    <main class="container-fluid mr-0">
        <div class="row application-content-row">
            <nav class="col-12 col-lg-2 position-sticky d-flex flex-column py-3 application-nav-column collapsed-menu">
                <div class="d-flex flex-column justify-content-center align-items-center">
                    <img class="w-100 logo" src="~/assets/images/logo.svg" alt="Massive Accessible Workflow Platform Logo"/>
                    <NuxtLink to="/" class="d-flex flex-row flex-lg-column mb-2 mb-lg-0 text-decoration-none">
                        <img class="w-100 logo" src="~/assets/images/logo.svg" alt="Massive Accessible Workflow Platform Logo"/>
                        <div>
                            <h1 class="brand text-break">
                                MAcWorP
                            </h1>
                            <small>Massive Accessible Workflow Platform</small>
                        </div>
                    </NuxtLink>
                </div>
                <div class="flex-fill"> </div>
                <div class="external-pages-menu">
                    <ul class="nav flex-column">
                        <li><a href="https://www.ruhr-uni-bochum.de/de/impressum" target="_blank">Legal information</a></li>
                        <li><a href="https://www.ruhr-uni-bochum.de/de/datenschutz" target="_blank">Privacy</a></li>
                    </ul>
                    <div class="sponsors-logos">
                        <a href="https://www.ruhr-uni-bochum.de" target="_blank"><img class="w-25" src="~/assets/images/rub-logo-small.png" alt="Logo of the Ruhr University Bochum"/></a>
                        <a href="https://www.cubimed.ruhr-uni-bochum.de" target="_blank"><img class="w-50" src="~/assets/images/cubimed-logo.png" alt="Core Unit Bioinformatics Ruhr University Bochum"/></a>
                    </div>
                </div>
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
    //  middleware: [
    //     "validate_login"
    // ]
}
</script>
