window.addEventListener('DOMContentLoaded', init);

function init() {
    let [submit, password, error, parent] = ["recovery-page-submit", "recovery-page-password", "recovery-page-error", "recovery-page-error-parent"].map(x => document.getElementById(x));
    submit.addEventListener("click", () => {

        let parts = window.location.href.split("/");
        let url = "/recovery/" + parts[parts.length - 1];
        console.log(url);

        fetch(url, {
            'method': 'POST',
            'headers': {
                'Content-Type': 'application/json'
            },
            'body': JSON.stringify(password.value)
        }).then(res => res.text().then(txt => {
            if(txt == "\"ResetSuccess\"") {
                parent.classList.remove("disabled");
                error.innerHTML = "Password changed";
                error.classList.remove("error");
                error.classList.add("ok");
                setTimeout(() => {
                    window.open("https://www.pixeart.online", "_self");
                }, 4000);
            }
            else if(txt == "\"ResetFailed\"") {
                parent.classList.remove("disabled");
                error.innerHTML = "Invalid password";
                error.classList.add("error");
                error.classList.remove("ok");
            }
        }));

    });
}