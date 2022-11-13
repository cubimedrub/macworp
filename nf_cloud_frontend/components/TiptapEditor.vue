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
            content: '\n' +
                '        <p>\n' +
                '          That’s a boring paragraph followed by a fenced code block:\n' +
                '        </p>\n' +
                '        <pre><code class="language-json">for (var i=1; i <= 20; i++)\n' +
                '{\n' +
                '  if (i % 15 == 0)\n' +
                '    console.log("FizzBuzz");\n' +
                '  else if (i % 3 == 0)\n' +
                '    console.log("Fizz");\n' +
                '  else if (i % 5 == 0)\n' +
                '    console.log("Buzz");\n' +
                '  else\n' +
                '    console.log(i);\n' +
                '}</code></pre>\n' +
                '        <p>\n' +
                '          Press Command/Ctrl + Enter to leave the fenced code block and continue typing in boring paragraphs.\n' +
                '        </p>\n' +
                '      ',
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
