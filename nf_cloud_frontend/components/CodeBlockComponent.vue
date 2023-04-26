<template>
    <node-view-wrapper class="code-block">
        <code><node-view-content /></code>
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

</style>
