<template>
  <div class="d-flex justify-content-center">
    Logging in...
    <Spinner class="d-flex justify-content-center"></Spinner>
  </div>
</template>

<script>
/**
 * Callback for the backend to send the JWT token to the frontend.
 */
export default {
    layout: "login",
    mounted(){
        this.get_token_and_redirect()
    },
    methods: {
      /**
       * Sets the JWT token.
       */
        get_token_and_redirect(){
            this.$store.commit("login/setJwt", this.$route.query.token)
            this.$router.replace({name: "projects"})
        }
    }
}
</script>

<style>

</style>