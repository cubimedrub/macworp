export default class ApiServerError {
    constructor(response) {
        this.response = response
    }

    async html(){
        return this.response.text()
        .then(response_body => {
            return response_body;
        })
    }
}