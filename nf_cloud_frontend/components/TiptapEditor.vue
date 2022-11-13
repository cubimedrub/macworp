<template>
    <editor-content :editor="editor" />

</template>

<script>
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight'
import Document from '@tiptap/extension-document'
import Paragraph from '@tiptap/extension-paragraph'
import Text from '@tiptap/extension-text'
import { Editor, EditorContent, VueNodeViewRenderer } from '@tiptap/vue-2'
import StarterKit from '@tiptap/starter-kit'
import json from 'highlight.js/lib/languages/json'
import { lowlight } from 'lowlight'
import CodeBlockComponent from './CodeBlockComponent'
lowlight.registerLanguage('json', json)
export default {
    components: {
        EditorContent,
    },
    data() {
        return {
            editor: null,
        }
    },
    mounted() {
        this.editor = new Editor({
            content:
                '<pre><code class="language-json">' +
                '{\n' +
                '    "type": "doc",\n' +
                '    "content": [\n' +
                '        {\n' +
                '            "type": "paragraph",\n' +
                '            "content": [\n' +
                '                {\n' +
                '                    "type": "text",\n' +
                '                    "text": "Wow, this editor instance exports its content as JSON."\n' +
                '                 }\n' +
                '         }</code></pre>\n',
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
                    .configure({ lowlight }),
            ],
        })
    },
    beforeDestroy() {
        this.editor.destroy()
    },
}
</script>
<style lang="scss">
/* Basic editor styles */
.ProseMirror {
    > * + * {
        margin-top: 0.75em;
    }
    pre {
        background: #0D0D0D;
        color: #FFF;
        font-family: 'JetBrainsMono', monospace;
        padding: 0.75rem 1rem;
        border-radius: 0.5rem;
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
            color: #B9F18D;
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
}
</style>
