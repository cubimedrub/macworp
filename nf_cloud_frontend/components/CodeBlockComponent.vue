<template>
    <node-view-wrapper class="code-block">
        <pre><code><node-view-content /></code></pre>
    </node-view-wrapper>
</template>

<script>
import { NodeViewContent, nodeViewProps, NodeViewWrapper } from '@tiptap/vue-2'
export default {
    components: {
        NodeViewWrapper,
        NodeViewContent,
    },
    props: nodeViewProps,
    data() {
        return {
            languages: this.extension.options.lowlight.listLanguages(),
        }
    },
    computed: {
        selectedLanguage: {
            get() {
                return this.node.attrs.language
            },
            set(language) {
                this.updateAttributes({ language })
            },
        },
    },
}
</script>

<style lang="scss">
.code-block {
    position: relative;
    select {
        position: absolute;
        top: 0.5rem;
        right: 0.5rem;
    }
}
</style>
