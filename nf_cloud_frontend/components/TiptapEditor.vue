<template>
    <editor-content :editor="editor"/>

</template>

<script>
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight'
import Document from '@tiptap/extension-document'
import Paragraph from '@tiptap/extension-paragraph'
import Text from '@tiptap/extension-text'
import {Editor, EditorContent, VueNodeViewRenderer} from '@tiptap/vue-2'
import StarterKit from '@tiptap/starter-kit'
import json from 'highlight.js/lib/languages/json'
import {lowlight} from 'lowlight'
import CodeBlockComponent from './CodeBlockComponent'

lowlight.registerLanguage('json', json)
export default {
    components: {
        EditorContent,
    },
    props: {
        value: {
            type: String,
            default: '',
        },
    },
    data() {
        return {
            editor: null,
        }
    },
    watch: {
        value(value) {
            // HTML
            const isSame = this.editor.getHTML() === value

            if (isSame) {
                return
            }

            this.editor.commands.setContent(value, false)
        },
    },
    mounted() {
        this.editor = new Editor({
            extensions: [
                StarterKit,
                Document,
                Paragraph,
                Text,
                CodeBlockLowlight
                    .extend({
                        addNodeView() {
                            return VueNodeViewRenderer(CodeBlockComponent)
                        },
                    })
                    .configure({lowlight}),
            ],
            content: this.value,
            onUpdate: () => {
                // HTML
                this.$emit('input', this.editor.getHTML())

                // JSON
                // this.$emit('input', this.editor.getJSON())
            },
        })
    },
    beforeDestroy() {
        this.editor.destroy()
    },
}
</script>
<style lang="scss">
/* Basic editor styles */
/* remove outline */

.ProseMirror {
    background: none;
    border-radius: 0.3em;
    font-size: 0.8rem;
    border: 1px solid #ccc;
    color: inherit;;
    font-family: 'JetBrainsMono', monospace;
    padding: 0;

    code {
        color: inherit;
        padding: 0;
        background: none;
        font-size: 0.8rem;
    }

    .hljs-comment,
    .hljs-quote {
        color: #616161;
    }

    .hljs-variable,
    .hljs-template-variable,
    .hljs-attribute,
    .hljs-tag,
    .hljs-name,
    .hljs-regexp,
    .hljs-link,
    .hljs-name,
    .hljs-selector-id,
    .hljs-selector-class {
        color: #F98181;
    }

    .hljs-number,
    .hljs-meta,
    .hljs-built_in,
    .hljs-builtin-name,
    .hljs-literal,
    .hljs-type,
    .hljs-params {
        color: #FBBC88;
    }

    .hljs-string,
    .hljs-symbol,
    .hljs-bullet {
        color: forestgreen;
    }

    .hljs-title,
    .hljs-section {
        color: #FAF594;
    }

    .hljs-keyword,
    .hljs-selector-tag {
        color: #70CFF8;
    }

    .hljs-emphasis {
        font-style: italic;
    }

    .hljs-strong {
        font-weight: 700;
    }

}
</style>
