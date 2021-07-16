<template>
    <div class="error-page">
        <h1>Errors</h1>
        <small>
            <button v-if="show_navigate_back_button" @click="navigateBack" type="button" class="btn btn-link btn-small">
                <i class="fas fa-chevron-left"></i>
                back to application
            </button>
        </small>
        <p>
            If you encounter an error multiple times, it would be great if you would file an issue on <a target="_blank" href="https://github.com/mpc-bioinformatics/">https://github.com/mpc-bioinformatics/</a>.
            So the developer are aware of it and can track it down.
        </p>
        <p>
            Please enter as many information as you have:
            <ul>
                <li>Error description from this site</li>
                <li>URL</li>
                <li>Entered parameters</li>
                <li>...</li>
            </ul>
        </p>
        <div class="list-group">
            <div v-for="error, idx in errors" :key="`${idx}-${error.title}`" :class="{'bg-danger': is_new_error && idx == 0}" class="list-group-item list-group-item-action flex-column align-items-start">
                <div class="d-flex w-100 justify-content-between">
                    <span>
                        <h5 class="mb-1">{{ error.title }}</h5>
                        <small>{{ error.date }}</small>
                    </span>
                    <button @click="removeError(idx)" type="button" class="btn btn-link btn-sm h-min-content">
                        <i class="fas fa-times"></i>
                    </button>
                </div>
                <p class="mb-1">{{ error.description }}</p>
            </div>
        </div>
    </div>
</template>

<script>
export default {
    data(){
        return {
            is_new_error: false
        }
    },
    activated(){
        if(this.$route.query.is_new_error){
            this.is_new_error = true
        }
    },
    methods: {
        removeError(idx){
            this.$store.commit("errors/remove", idx)
        },
        navigateBack(){
            this.$router.go(-1)
        }
    },
    computed: {
        errors(){
            return this.$store.state.errors.errors
        },
        show_navigate_back_button(){
            return this.is_new_error && this.errors.length
        }
    }
}
</script>