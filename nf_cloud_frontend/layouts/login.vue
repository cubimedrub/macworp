<template>
    <main class="container-fluid mr-0">
        <div class="row application-content-row">
            <nav class="col-12 col-lg-2 position-sticky d-flex flex-column py-3 application-nav-column collapsed-menu">
                <div class="d-flex justify-content-between align-items-center">
                    <h1 class="brand text-break">
                        <NuxtLink to="/">
                            Nextflow Cloud
                        </NuxtLink>
                    </h1>
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
