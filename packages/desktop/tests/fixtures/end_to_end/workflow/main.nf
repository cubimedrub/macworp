nextflow.enable.dsl = 2

// End-to-end test workflow. Receives the same parameters twice, once as fixed parameters
// (prefix `fixed_`) and once as dynamic ones (prefix `dyn_`).
//
// * writes every parameter value into results/params.txt
// * overwrites every given file with the test token
// * replaces `old.txt` in every given folder with `<token>.marker`
// * overwrites every file matched by the glob with the test token

params.token = ''

process report {
    publishDir 'results', mode: 'copy'

    input:
    val content

    output:
    path 'params.txt'

    script:
    """
cat > params.txt <<'MACWORP_EOF'
${content}
MACWORP_EOF
    """
}

def splitPaths(value) {
    return value.toString().split(',').findAll { it }
}

workflow {
    def token = params.token
    if (!token) {
        error 'token is required'
    }

    def lines = []
    ['fixed', 'dyn'].each { prefix ->
        def text = params["${prefix}_text"]
        def multiline = params["${prefix}_multiline"]
        def integer = params["${prefix}_integer"]
        def decimal = params["${prefix}_float"]
        def glob = params["${prefix}_glob"]
        def singleFile = params["${prefix}_file"]
        def files = params["${prefix}_files"]
        def folder = params["${prefix}_folder"]
        def folders = params["${prefix}_folders"]
        def select = params["${prefix}_select"]
        def multiselect = params["${prefix}_multiselect"]

        def matches = [file(glob)].flatten().sort { it.name }
        matches.each { it.text = token }

        (splitPaths(singleFile) + splitPaths(files)).each { file(it).text = token }
        (splitPaths(folder) + splitPaths(folders)).each {
            file("${it}/old.txt").delete()
            file("${it}/${token}.marker").text = token
        }

        lines << "${prefix}_text => ${text}"
        lines << "${prefix}_multiline => ${multiline}"
        lines << "${prefix}_integer => ${integer}"
        lines << "${prefix}_float => ${decimal}"
        lines << "${prefix}_glob => ${glob}"
        lines << "${prefix}_glob_matches => ${matches.collect { it.name }.join(',')}"
        lines << "${prefix}_file => ${singleFile}"
        lines << "${prefix}_files => ${files}"
        lines << "${prefix}_folder => ${folder}"
        lines << "${prefix}_folders => ${folders}"
        lines << "${prefix}_select => ${select}"
        lines << "${prefix}_multiselect => ${multiselect}"
    }
    lines << "token => ${token}"

    report(lines.join('\n'))
}
