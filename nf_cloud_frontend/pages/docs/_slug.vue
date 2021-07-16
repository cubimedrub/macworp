<template>
    <div>
        <h1>{{ article.title }}</h1>
        <p>{{ article.description }}</p>
        <nav>
            <ul>
                <li v-for="link of article.toc" :key="link.id" :style="`margin-left: ${15 * (link.depth - toc_offset)}px`">
                    <NuxtLink :to="`#${link.id}`">{{ link.text }}</NuxtLink>
                </li>
            </ul>
        </nav>
        <article>
            <nuxt-content :document="article"></nuxt-content>
        </article>
    </div>
</template>

<script>
export default {
    async asyncData({ $content, params }) {
        const article = await $content('docs', params.slug).fetch()
        return { article }
    },
    computed: {
        toc_offset(){
            /* 
             * TOC depth is defined by the header level (h1, h2, ...) respectively by the number of hashes in markdown.
             * This method returns depth where the TOC starts.
             */
            if(this.article){
                return Math.min.apply(Math, this.article.toc.map(menu_item => menu_item.depth))
            } else {
                return 0
            }
        }
    }
}
</script>