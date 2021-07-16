export default class ApiClientError {
    constructor(response) {
        this.response = response
    }

    async errors(){
        return this.response.json()
        .then(response_body => {
            return response_body.errors;
        })
    }
}