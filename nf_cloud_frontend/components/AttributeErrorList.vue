<template>
    <ul v-if="errors.length > 0">
        <li v-for="error in errors" :key="error">
            {{ error }}
        </li>
    </ul>
</template>

<script>
export default {
    props: {
        errors: {
            type: Array,
            required: true
        }
    }
}
</script>