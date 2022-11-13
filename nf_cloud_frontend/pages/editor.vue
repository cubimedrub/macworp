<template>
    <div>
        <client-only>
            <tiptap-editor />
            <div>
                <a class="btn btn-primary">
                    Export
                </a>
                <a class="btn btn-primary">
                    Import
                </a>
                <a class="btn btn-primary">
                    Save
                </a>
                <NuxtLink to="/projects" class="nav-link">
                    <a class="btn btn-outline-secondary">
                        Cancel
                    </a>
                </NuxtLink>
            </div>

        </client-only>
    </div>
</template>
<script>
import TiptapEditor from '~/components/TiptapEditor.vue'
export default {
    components: {
        TiptapEditor
    },
}
</script>
<style>
.ProseMirror{
    background-color: darkgrey;
    border-radius: 0.5rem;
    font-size: 1.5rem;
}
</style>
