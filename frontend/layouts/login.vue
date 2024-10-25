<template>
    <main class="container-fluid mr-0">
        <div class="row application-content-row">
            <nav class="col-12 col-lg-2 position-sticky d-flex flex-column py-3 application-nav-column collapsed-menu">
                <div class="d-flex flex-column justify-content-center align-items-center">
                    <NuxtLink to="/" class="text-decoration-none">
                        <h1 class="brand text-break">
                            MAcWorP
                        </h1>
                        <small>Massive Accessible Workflow Platform</small>
                    </NuxtLink>
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
