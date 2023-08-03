<template>
    <div class="d-flex justify-content-between">
        <hr class="w-100">
        <div class="ds-flex align-self-center px-3">{{ label }}</div>
        <hr class="w-100">
    </div>
</template>

<script>
export default {
    props: {
        label: {
            type: String,
            required: true
        }
    }
}
</script>