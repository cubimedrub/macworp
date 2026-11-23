nextflow.enable.dsl = 2

// End-to-end test workflow for engine parameters: the value of `params.greeting` comes from the
// profile `ci` (see nextflow.config) that the test selects with the engine parameter `profile`.

process greet {
    publishDir 'results', mode: 'copy'

    output:
    path 'profile.txt'

    script:
    """
    echo '${params.greeting}' > profile.txt
    """
}

workflow {
    greet()
}
