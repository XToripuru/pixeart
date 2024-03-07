window.addEventListener('DOMContentLoaded', init);

let board = {
    grid: {
        w: 1000,
        h: 1000
    },
    offset: {
        x: 0,
        y: 0
    },
    scale: 8,
    pixels: null,
    img: null,
    render: true,
    update: true
}

let net = {
    socket: null,
    connected: false,
    receive: {
        "LoginFailed": () => {
            let [parent, label] = ["login-page-error-parent", "login-page-error"].map(x => document.getElementById(x));
            parent.classList.remove("disabled");
            label.innerHTML = "Invalid login or password";
        },
        "LoginSuccess": (value) => {
            account.logged = true;
            account.name = value[0];
            account.email = value[1];
            account.tier = value[2] == "Free" ? 0 : value[2]["Tier"];
            account.last = value[3] * 1000;
            account.verified = value[4];

            document.getElementById("welcome").innerHTML = "Welcome, " + account.name + "!";

            for(let k = 0; k<account.tier; k++) {
                let card = document.getElementById("tier-" + k);
                card.classList.add("disabled");
            }
            {
                let price = document.getElementById("tier-" + account.tier + "-price");
                let button = document.getElementById("tier-" + account.tier + "-button");
                price.innerHTML = "&mdash;";
                button.innerHTML = "Current";
            }
            for(let k = account.tier + 1; k<6; k++) {
                let price = document.getElementById("tier-" + k + "-price");
                price.innerHTML = "$" + (account.price(k) - account.price(account.tier));
            }

            if(!account.verified) {
                document.getElementById("verify-page-text").innerHTML = "Verification link was sent to<br/>" + account.email + "<br/>(check spam too)";
                pageToggle("verify-page");
            } else {
                navToggle();
                pageToggle("account-page");
                document.getElementById("meta").classList.remove("disabled");
                document.getElementById("timer").classList.remove("disabled");
                document.getElementById("position").classList.remove("disabled");
            }
        },
    
        "RegistrationFailed": (value) => {
            let [parent, label] = ["register-page-error-parent", "register-page-error"].map(x => document.getElementById(x));
            parent.classList.remove("disabled");
            console.log(value);
            label.innerHTML = "Invalid login or password or email";
            if(value == "BadEmail") {
                label.innerHTML = "Invalid email";
            }
            if(value == "BadLogin") {
                label.innerHTML = "Invalid login";
            }
            if(value == "BadPassword") {
                label.innerHTML = "Invalid password";
            }
            if(value == "LoginTaken") {
                label.innerHTML = "Username taken";
            }
            if(value == "EmailTaken") {
                label.innerHTML = "Email taken";
            }
        },
        "RegistrationSuccess": (value) => {
            account.logged = true;
            account.tier = value[0] == "Free" ? 0 : value[0]["Tier"];
            account.last = value[1] * 1000;
            document.getElementById("welcome").innerHTML = "Welcome, " + account.name + "!";
            document.getElementById("verify-page-text").innerHTML = "Verification link was sent to<br/>" + account.email + "<br/>(check spam too)";
            pageToggle("verify-page");
            // pageToggle("account-page");
            // document.getElementById("meta").classList.toggle("disabled");
            // document.getElementById("timer").classList.toggle("disabled");
            // document.getElementById("position").classList.toggle("disabled");
        },
    
        "AlreadyConnected": () => {
    
        },
        "AnotherSession": () => {
    
        },
        "Unexpected": () => {
    
        },
    
        "Queue": (value) => {
            for(let k = 0; k<value.length; k++) {
                let [r, g, b, idx] = [value[k][0][0], value[k][0][1], value[k][0][2], value[k][1]];
                board.pixels[4 * idx + 0] = r;
                board.pixels[4 * idx + 1] = g;
                board.pixels[4 * idx + 2] = b;
            }
            board.update = true;
        },
    
        "PixelUpdateSuccess": () => {
            account.last = Date.now();
        },
        "PixelUpdateFailed": () => {
            
        },

        "BadEmail": () => {
            let [parent, label] = ["lostpass-page-error-parent", "lostpass-page-error"].map(x => document.getElementById(x));
            parent.classList.remove("disabled");
            label.innerHTML = "Invalid email";
            label.classList.add("error");
            label.classList.remove("ok");
        },
        "EmailSent": () => {
            let [parent, label] = ["lostpass-page-error-parent", "lostpass-page-error"].map(x => document.getElementById(x));
            parent.classList.remove("disabled");
            label.innerHTML = "Email sent";
            label.classList.remove("error");
            label.classList.add("ok");
        },

        "VerifySuccess": () => {
            account.verified = true;
            pageToggle("account-page");
            document.getElementById("meta").classList.remove("disabled");
            document.getElementById("timer").classList.remove("disabled");
            document.getElementById("position").classList.remove("disabled");
        },

        "BuyLink": (value) => {
            window.open(value, "_blank").focus();
        },
        "TierChange": (value) => {
            console.log(value);
            account.tier = value == "Free" ? 0 : value["Tier"];

            for(let k = 0; k<account.tier; k++) {
                let card = document.getElementById("tier-" + k);
                card.classList.add("disabled");
            }
            {
                let price = document.getElementById("tier-" + account.tier + "-price");
                let button = document.getElementById("tier-" + account.tier + "-button");
                price.innerHTML = "&mdash;";
                button.innerHTML = "Current";
            }
            for(let k = account.tier + 1; k<6; k++) {
                let price = document.getElementById("tier-" + k + "-price");
                price.innerHTML = "$" + (account.price(k) - account.price(account.tier));
            }
        },

        "TooMany": (value) => {
            if(value == "Register") {
                let [parent, label] = ["register-page-error-parent", "register-page-error"].map(x => document.getElementById(x));
                parent.classList.remove("disabled");
                label.innerHTML = "Wait before creating new account";
            }
            if(value == "Recovery") {
                let [parent, label] = ["lostpass-page-error-parent", "lostpass-page-error"].map(x => document.getElementById(x));
                parent.classList.remove("disabled");
                label.innerHTML = "Wait before generating new link";
            }
        }
    }
};

let account = {
    logged: false,
    name: null,
    tier: null,
    last: -Infinity,
    verified: false,
    cooldown: () => {
        switch(account.tier) {
            case 0: return 180;
            case 1: return 60;
            case 2: return 30;
            case 3: return 15;
            case 4: return 5;
            case 5: return 1;
            default: return 360;
        }
    },
    price: (tier) => {
        switch(tier) {
            case 0: return 0;
            case 1: return 2;
            case 2: return 4;
            case 3: return 8;
            case 4: return 20;
            case 5: return 60;
            default: return 0;
        }
    }
};

let picker = {
    picked: { r: 0, g: 0, b: 0 },
    render: true,
    hue: [
        [255, 0, 0, 0],
        [255, 85, 0, 20],
        [255, 145, 0, 34],
        [255, 255, 0, 60],
        [157, 255, 0, 83],
        [0, 255, 0, 120],
        [0, 255, 162, 158],
        [0, 255, 255, 180],
        [0, 162, 255, 202],
        [0, 72, 255, 223],
        [0, 0, 255, 240],
        [85, 0, 255, 260],
        [157, 0, 255, 277],
        [255, 0, 255, 300],
        [255, 0, 183, 317],
        [255, 0, 119, 332]
    ]
}

function init() {
    setTimeout(connect, 0);
    setTimeout(startup, 0);
    setTimeout(draw, 0);
}

function connect() {
    console.log("connecting");
    net.socket = new WebSocket('wss://pixeart.online/ws');

    net.socket.addEventListener("open", (event) => {
        console.log("connected");
        net.connected = true;
        document.getElementById("nav").classList.toggle("disabled");
        document.getElementById("site-content").classList.toggle("disabled");
        document.getElementById("connecting-page").classList.toggle("disabled");
        document.getElementById("connecting-page").classList.toggle("connecting-page");
    });

    net.socket.addEventListener("close", (event) => {
        if(net.connected) {
            console.log("close first time");
            // document.getElementById("nav").classList.toggle("disabled");
            // document.getElementById("site-content").classList.toggle("disabled");
            // document.getElementById("connecting-page").classList.toggle("disabled");
            // document.getElementById("connecting-page").classList.toggle("connecting-page");
            net.connected = false;
        } else {
            console.log("close retry");
        }
        net.socket = null;
        //setTimeout(connect, 0);
    });

    net.socket.addEventListener("message", (event) => {
        if(event.data instanceof Blob) {
            event.data.arrayBuffer().then(buffer => {
                const parsed = new Uint8ClampedArray(buffer);
                board.pixels = new Uint8ClampedArray(4 * board.grid.w * board.grid.h);
                for(let k = 0; k<board.grid.w * board.grid.h; k++) {
                    board.pixels[k * 4 + 0] = parsed[k * 3 + 0];
                    board.pixels[k * 4 + 1] = parsed[k * 3 + 1];
                    board.pixels[k * 4 + 2] = parsed[k * 3 + 2];
                    board.pixels[k * 4 + 3] = 255;
                }
                board.img = new ImageData(board.pixels, board.grid.w, board.grid.h);
            });
        } else {
            let parsed = JSON.parse(event.data);

            let type = Object.keys(parsed)[0];
            
            if(type == 0) {
                net.receive[parsed]();
            } else {
                let value = Object.values(parsed)[0];
                net.receive[type](value);
            }
        }

    });
}

function login() {
    let [login, password] = ["login-page-login", "login-page-password"].map(x => document.getElementById(x));
    account.name = login.value;
    net.socket.send(JSON.stringify({
        'Login': {
            'username': login.value,
            'password': password.value
        }
    }));
}

function register() {
    let [login, password, email, checkbox] = [
        "register-page-login",
        "register-page-password",
        "register-page-email",
        "register-page-checkbox"
    ].map(x => document.getElementById(x));
    let [parent, label] = ["register-page-error-parent", "register-page-error"].map(x => document.getElementById(x));
    if(!checkbox.checked) {
        parent.classList.remove("disabled");
        label.innerHTML = "You must agree to the terms and conditions";
        return;
    }
    account.name = login.value;
    account.email = email.value;
    if(account.name.length == 0) {
        parent.classList.remove("disabled");
        label.innerHTML = "Name is too short";
        return;
    }
    if(account.name.length > 32) {
        parent.classList.remove("disabled");
        label.innerHTML = "Name is too long";
        return;
    }

    if(password.value.length < 8) {
        parent.classList.remove("disabled");
        label.innerHTML = "Password is too short";
        return;
    }
    if(password.value.length > 32) {
        parent.classList.remove("disabled");
        label.innerHTML = "Password is too long";
        return;
    }

    if(account.email.length == 0) {
        parent.classList.remove("disabled");
        label.innerHTML = "Email is too short";
        return;
    }
    if(account.email.length > 64) {
        parent.classList.remove("disabled");
        label.innerHTML = "Email is too long";
        return;
    }

    net.socket.send(JSON.stringify({
        'Register': {
            'username': login.value,
            'password': password.value,
            'email': email.value
        }
    }));
}

function lostpass() {
    let [email] = ["lostpass-page-email"].map(x => document.getElementById(x));
    net.socket.send(JSON.stringify({
        'Recovery': email.value
    }));
}

function pageToggle(page) {
    document.getElementById("login-page").classList.toggle("disabled", page != "login-page");
    document.getElementById("register-page").classList.toggle("disabled", page != "register-page");
    document.getElementById("lostpass-page").classList.toggle("disabled", page != "lostpass-page");
    document.getElementById("account-page").classList.toggle("disabled", page != "account-page");
    document.getElementById("verify-page").classList.toggle("disabled", page != "verify-page");
}

function navToggle() {
    document.querySelector('body').classList.toggle('nav-active');
	document.querySelector('.menu-icon').classList.toggle('blend');
}

function startup() {

    {
        let [logout] = ["log-out"].map(x => document.getElementById(x));
        document.getElementById("meta").classList.add("disabled");
        document.getElementById("timer").classList.add("disabled");
        document.getElementById("position").classList.add("disabled");
        logout.addEventListener("click", () => {
            net.socket.send(JSON.stringify("Logout"));
            account.logged = false;
            pageToggle("login-page");
        });

        for(let k = 1; k<6; k++) {
            let button = document.getElementById("tier-" + k + "-button");
            button.addEventListener("click", () => {
                console.log(k, account.tier);
                if(k > account.tier) {
                    net.socket.send(JSON.stringify({
                        'Buy': k
                    }));
                }
            });
        }
    }

    {
        let [_login, password, forgot, submit, signup] = ["login-page-login", "login-page-password", "login-page-forgot", "login-page-submit", "login-page-sign-up"].map(x => document.getElementById(x));
        _login.addEventListener("keydown", (event) => {
            if(event.key === "Enter") login();
        });
        password.addEventListener("keydown", (event) => {
            if(event.key === "Enter") login();
        });
        forgot.addEventListener("click", () => {
            pageToggle("lostpass-page");
        });
        submit.addEventListener("click", () => {
            login();
        });
        signup.addEventListener("click", () => {
            pageToggle("register-page");
        });
    }

    {
        let [login, password, email, checkbox, submit, signin] = [
            "register-page-login",
            "register-page-password",
            "register-page-email",
            "register-page-checkbox",
            "register-page-submit",
            "register-page-sign-in"
        ].map(x => document.getElementById(x));
        login.addEventListener("keydown", (event) => {
            if(event.key === "Enter") register();
        });
        password.addEventListener("keydown", (event) => {
            if(event.key === "Enter") register();
        });
        email.addEventListener("keydown", (event) => {
            if(event.key === "Enter") register();
        });
        submit.addEventListener("click", () => {
            register();
        });
        signin.addEventListener("click", () => {
            pageToggle("login-page");
        });
    }

    {
        let [submit, signin] = [
            "lostpass-page-submit",
            "lostpass-page-sign-in"
        ].map(x => document.getElementById(x));
        submit.addEventListener("click", () => {
            lostpass();
        });
        signin.addEventListener("click", () => {
            pageToggle("login-page");
        });
    }

    document.addEventListener("contextmenu", (event) => {
        event.preventDefault();
    });

    body = document.querySelector('body');
	hitbox = document.querySelector('.menu-icon-hitbox');
    menu = document.querySelector('.menu-icon');
	menuItems = document.querySelectorAll('.nav__list-item');

    hitbox.addEventListener('click', () => {
        body.classList.toggle('nav-active');
        menu.classList.toggle('blend');
    });
}

function draw() {
    console.log("draw");
    const main = document.getElementsByClassName("main")[0];

    const canvas = document.getElementById("canvas");
    canvas.style.position = "relative";
    canvas.width = 1000;
    canvas.height = 1000;

    const overlay = document.getElementById("overlay");
    overlay.style.position = "absolute";
    overlay.width = 16 * 18;
    overlay.height = 16 * 20;

    let ctx = {
        canvas: canvas.getContext("2d"),
        overlay: overlay.getContext("2d")
    };

    ctx.canvas.imageSmoothingEnabled = false;


    let mouse = {
        curr: { x: 0, y: 0, btn: 0, pressed: false },
        prev: { x: 0, y: 0, btn: 0, pressed: false },
        asyn: { x: 0, y: 0, btn: 0, pressed: false }
    };

    document.addEventListener('mousemove', (event) => {
        mouse.asyn.x = event.clientX;
        mouse.asyn.y = event.clientY;
        mouse.asyn.btn = event.buttons;
    });
    document.addEventListener('mouseup', (event) => {
        mouse.asyn.x = event.clientX;
        mouse.asyn.y = event.clientY;
        mouse.asyn.btn = event.buttons;
        mouse.asyn.pressed = false;
    });
    document.addEventListener('mousedown', (event) => {
        mouse.asyn.x = event.clientX;
        mouse.asyn.y = event.clientY;
        mouse.asyn.btn = event.buttons;
        mouse.asyn.pressed = true;
    });
    document.addEventListener("wheel", (event) => {
        let delta = Math.sign(event.deltaY);
        board.scale -= (board.scale * delta) * 0.1;
        if(board.scale < 0.5) {
            board.scale = 0.5;
        } else if(board.scale > 64.0) {
            board.scale = 64.0;
        }
    });
    document.addEventListener('keydown', (event) => {
        if(event.key == "Shift") {
            //picker.render = true;
            overlay.classList.toggle("disabled");
        }
    });
    document.addEventListener('keyup', (event) => {
        if(event.key == "Shift") {
            //picker.render = false;
        }
    });

    // Board
    let a_scale = board.scale;

    // Picker
    let h = 0;
    let a_h = 0;
    let hold_h = false;

    let s = 0;
    let a_s = 0;
    let b = 0;
    let a_b = 0;
    let hold_sb = false;

    const timer = {
        box: document.getElementById("timer-box"),
        text: document.getElementById("timer"),
    };
    const position = document.getElementById("position");

    setInterval(() => {
        if(board.img == null) {
            console.log("image null");
            return;
        }

        mouse.prev = { ...mouse.curr };
        mouse.curr = { ...mouse.asyn };
        
        let bound = main.getBoundingClientRect();

        if(board.render) {
            ctx.canvas.putImageData(board.img, 0, 0);
            
            if(mouse.curr.pressed && mouse.curr.btn === 2) {
                board.offset.x += (mouse.curr.x - mouse.prev.x) / a_scale;
                board.offset.y += (mouse.curr.y - mouse.prev.y) / a_scale;
                if(board.offset.x < -500) board.offset.x = -500;
                if(board.offset.x > 500) board.offset.x = 500;
                if(board.offset.y < -500) board.offset.y = -500;
                if(board.offset.y > 500) board.offset.y = 500;
            }
    
            a_scale += (board.scale - a_scale) * 0.2;
            if(Math.abs(a_scale - board.scale) < board.scale * 0.02) {
                a_scale = board.scale;
            }
    
            canvas.style.transform = "scale(" + a_scale + "," + a_scale + ")";
            canvas.style.left = "calc(50vw - " + (canvas.width * 0.5) + "px + " + (board.offset.x * a_scale) + "px)";
            canvas.style.top = "calc(50vh - " + (canvas.height * 0.5) + "px + " + (board.offset.y * a_scale) + "px)";

            let x = Math.floor((mouse.curr.x - bound.width * 0.5) / a_scale - (board.offset.x - board.grid.w / 2));
            let y = Math.floor((mouse.curr.y - bound.height * 0.5) / a_scale - (board.offset.y - board.grid.h / 2));

            if(x >= 0 && y >= 0 && x < board.grid.w && y < board.grid.h) {
                position.innerHTML = "X" + x + " Y" + y;
            } else {
                position.innerHTML = "";
            }

            if(x >= 0 && y >= 0 && x < board.grid.w && y < board.grid.h) {
                ctx.canvas.fillStyle = "#000000C0";
                ctx.canvas.fillRect(x - 2, y - 2, 2, 1);
                ctx.canvas.fillRect(x + 1, y - 2, 2, 1);
                ctx.canvas.fillRect(x - 2, y + 2, 2, 1);
                ctx.canvas.fillRect(x + 1, y + 2, 2, 1);

                ctx.canvas.fillRect(x - 2, y - 1, 1, 1);
                ctx.canvas.fillRect(x - 2, y + 1, 1, 1);
                ctx.canvas.fillRect(x + 2, y - 1, 1, 1);
                ctx.canvas.fillRect(x + 2, y + 1, 1, 1);

                // const dist = 3;
                // const len = 3;
                // ctx.canvas.fillRect(Math.max(x - dist - len, 0), y, Math.min(len, x - dist), 1);

                // ctx.canvas.fillRect(Math.min(x + dist + 1, board.grid.w - 1), y, Math.max(0, Math.min(len, board.grid.w - x - dist - 1)), 1);
                
                // ctx.canvas.fillRect(x, Math.max(y - dist - len, 0), 1, Math.min(len, x - dist));

                // ctx.canvas.fillRect(x, Math.min(y + dist + 1, board.grid.h - 1), 1, Math.max(0, Math.min(len, board.grid.h - y - dist - 1)));
            }
            // ctx.canvas.fillRect(0, y, x, 1);
            // ctx.canvas.fillRect(x + 3, y, board.grid.w - x - 3, 1);
            
            // ctx.canvas.fillRect(x, 0, 1, y);
            // ctx.canvas.fillRect(x, y + 3, 3, board.grid.h - y - 1);

            if(mouse.curr.pressed
            && mouse.curr.btn === 1
            && x >= 0
            && x < board.grid.w
            && y >= 0
            && y < board.grid.h
            // color picker
            && (!picker.render || mouse.curr.x < bound.width - 16 * 18 || mouse.curr.y < bound.height - 16 * 20)
            // menu button
            && (mouse.curr.x < bound.width - 16 * 4 || mouse.curr.y > 16 * 4)
            && !document.querySelector('body').classList.contains('nav-active')
            ) {
                if(account.verified && account.last + account.cooldown() * 1000 <= Date.now()) {
                    let k = 4 * (y * 1000 + x);
                    board.pixels[k + 0] = picker.picked.r;
                    board.pixels[k + 1] = picker.picked.g;
                    board.pixels[k + 2] = picker.picked.b;
                    board.update = true;
    
                    // account.last = Date.now();
                    net.socket.send(JSON.stringify({
                        'SetPixel': {
                            'color': [picker.picked.r, picker.picked.g, picker.picked.b],
                            'idx': (y * 1000 + x)
                        }
                    }));
                } else if(!account.logged) {
                    navToggle();
                    pageToggle("login-page");
                    return;
                }
            }

        }


        ctx.overlay.clearRect(0, 0, overlay.width, overlay.height);
        if(picker.render) {
            overlay.style.left = "calc(100vw - " + (overlay.width) + "px)";
            overlay.style.top = "calc(100vh - " + (overlay.height) + "px)";
            //overlay.style.visibility = picker.render ? "visible" : "hidden";
            //overlay.classList.toggle("disabled");
        

            let rx = mouse.curr.x - bound.width + overlay.width;
            let ry = mouse.curr.y - bound.height + overlay.height;

            ctx.overlay.fillStyle = "#000000FF";
            ctx.overlay.fillRect(0, 0, overlay.width, overlay.height);

            for(let k = 0; k<16; k++) {
                ctx.overlay.fillStyle = "rgb(" + picker.hue[k][0] + "," + picker.hue[k][1] + "," + picker.hue[k][2] + ")";
                ctx.overlay.fillRect(16 * (1 + k), 16 * 18, 16, 16);
            }

            for(let k = 0; k<256; k++) {
                let [_h, _s, _b] = hsb2hsl(picker.hue[h][3], Math.floor(100 * (k % 16) / 15), Math.floor(100 * Math.floor(k / 16) / 16));
                ctx.overlay.fillStyle = "hsl(" + _h + " " + _s + "% " + _b + "%)";
                ctx.overlay.fillRect(16 * (1 + k % 16), 16 * (1 + Math.floor(k / 16)), 16, 16);
            }

            
            ctx.overlay.strokeStyle = "#FFFFFF"
            ctx.overlay.lineWidth = 2;
            ctx.overlay.strokeRect(Math.floor(16 * (1 + a_h) - 1), 16 * 18 - 1, 18, 18);

            ctx.overlay.strokeRect(Math.floor(16 * (1 + a_s) - 1), 16 * (1 + a_b) - 1, 18, 18);
            
            if(!hold_h && !mouse.prev.pressed && mouse.curr.pressed && rx > 16 && rx < 16 * 17 && ry > 16 * 18 && ry < 16 * 19) {
                hold_h = true;
            }

            if(!hold_sb && !mouse.prev.pressed && mouse.curr.pressed && rx > 16 && rx < 16 * 17 && ry > 16 && ry < 16 * 17) {
                hold_sb = true;
            }

            if(hold_h) {
                h = Math.floor((rx - 16) / 16);
                if(h < 0) h = 0;
                if(h > 16 - 1) h = 16 - 1;
                if(mouse.prev.pressed && !mouse.curr.pressed) {
                    hold_h = false;

                    let [__h, __s, __b] = hsb2hsl(picker.hue[h][3], Math.floor(100 * s / 15), Math.floor(100 * b / 15));
                    let [_r, _g, _b] = hsl2rgb(__h, __s, __b);
                    picker.picked.r = _r;
                    picker.picked.g = _g;
                    picker.picked.b = _b;
                }
            }

            if(hold_sb) {
                s = Math.floor((rx - 16) / 16);
                b = Math.floor((ry - 16) / 16);
                if(s < 0) s = 0;
                if(s > 16 - 1) s = 16 - 1;
                if(b < 0) b = 0;
                if(b > 16 - 1) b = 16 - 1;
                if(mouse.prev.pressed && !mouse.curr.pressed) {
                    hold_sb = false;

                    let [__h, __s, __b] = hsb2hsl(picker.hue[h][3], Math.floor(100 * s / 15), Math.floor(100 * b / 15));
                    let [_r, _g, _b] = hsl2rgb(__h, __s, __b);
                    picker.picked.r = _r;
                    picker.picked.g = _g;
                    picker.picked.b = _b;
                }
            }

            a_h += (h - a_h) * 0.2;
            if(Math.abs(h - a_h) < 0.1) {
                a_h = h;
            }

            a_s += (s - a_s) * 0.2;
            if(Math.abs(s - a_s) < 0.1) {
                a_s = s;
            }

            a_b += (b - a_b) * 0.2;
            if(Math.abs(b - a_b) < 0.1) {
                a_b = b;
            }
        }

        if(account.verified) {
            let time = Date.now() - account.last;
            const cd = account.cooldown();
            if(time < cd * 1000) {
                let width = 100 * (time / (cd * 1000));
                timer.box.style.left = (100 - width) + "%";
                timer.box.style.width = width + "%";
                if(cd * 1000 - time >= 60000) {
                    let secs = (cd - Math.floor(time / 1000));
                    let mins = Math.floor(secs / 60);
                    timer.text.innerHTML = mins + ":" + String(secs % 60).padStart(2, "0");
                } else if(cd * 1000 - time >= 5000) {
                    timer.text.innerHTML = (cd - Math.floor(time / 1000));
                } else {
                    timer.text.innerHTML = Number.parseFloat((cd * 1000 - time) / 1000.0).toFixed(1);
                }
            } else {
                timer.text.innerHTML = "";
                timer.box.style.left = "0%";
                timer.box.style.width = "100%";
            }
        }

    }, 10);
}

// function overlay() {
//     const main = document.getElementsByClassName("main")[0];
//     const canvas = document.getElementById("overlay");
    
//     canvas.width = 16 * 18;
//     canvas.height = 16 * 20;
//     canvas.style.position = "absolute";

//     let pmouse = { x: 0, y: 0, btn: 0, pressed: false };
//     let mouse = { x: 0, y: 0, btn: 0, pressed: false };
//     let amouse = { x: 0, y: 0, btn: 0, pressed: false };

//     document.addEventListener('mousemove', (event) => {
//         amouse.x = event.clientX;
//         amouse.y = event.clientY;
//         amouse.btn = event.buttons;
//     });
//     document.addEventListener('mouseup', (event) => {
//         amouse.pressed = false;
//     });
//     document.addEventListener('mousedown', (event) => {
//         amouse.pressed = true;
//     });
//     document.addEventListener('keydown', (event) => {
//         if(event.key == "Shift") {
//             show = 1;
//         }
//     });
//     document.addEventListener('keyup', (event) => {
//         if(event.key == "Shift") {
//             show = 0;
//         }
//     });

//     const ctx = canvas.getContext("2d");

//     let h = 0;
//     let a_h = 0;
//     let hold_h = false;

//     let s = 0;
//     let a_s = 0;
//     let b = 0;
//     let a_b = 0;
//     let hold_sb = false;

//     let show = 0;

//     const timer = document.getElementById("timer");
//     const timertext = document.getElementById("timer-text");

//     setInterval(() => {

//         pmouse = { ...mouse };
//         mouse = { ...amouse };

//         canvas.style.left = "calc(100vw - " + (canvas.width) + "px)";
//         canvas.style.top = "calc(100vh - " + (canvas.height) + "px)";
//         canvas.style.visibility = show === 1 ? "visible" : "hidden";
//         // canvas.style.left = "0px";
//         // canvas.style.top = "0px";

//         let bound = main.getBoundingClientRect();

//         let time = Date.now() - account.last;
//         const cd = cooldown();
//         if(time < cd * 1000) {
//             let timer_w = 100 * (time / (cd * 1000));
//             timer.style.left = (100 - timer_w) + "%";
//             timer.style.width = timer_w + "%";
//             if(cd * 1000 - time >= 5000) {
//                 timertext.innerHTML = (cd - Math.floor(time / 1000));
//             } else {
//                 timertext.innerHTML = Number.parseFloat((cd * 1000 - time) / 1000.0).toFixed(1);
//             }
//         } else {
//             timertext.innerHTML = "";
//             timer.style.left = "0%";
//             timer.style.width = "100%";
//         }

//         ctx.clearRect(0, 0, canvas.width, canvas.height);

//         let rx = mouse.x - bound.width + canvas.width;
//         let ry = mouse.y - bound.height + canvas.height;

//         ctx.fillStyle = "#000000FF";
//         ctx.fillRect(0, 0, canvas.width, canvas.height);

//         for(let k = 0; k<16; k++) {
//             ctx.fillStyle = "rgb(" + hue[k][0] + "," + hue[k][1] + "," + hue[k][2] + ")";
//             ctx.fillRect(16 * (1 + k), 16 * 18, 16, 16);
//         }

//         for(let k = 0; k<256; k++) {
//             let [_h, _s, _b] = hsb2hsl(hue[h][3], Math.floor(100 * (k % 16) / 15), Math.floor(100 * Math.floor(k / 16) / 16));
//             ctx.fillStyle = "hsl(" + _h + " " + _s + "% " + _b + "%)";
//             ctx.fillRect(16 * (1 + k % 16), 16 * (1 + Math.floor(k / 16)), 16, 16);
//         }

        
//         ctx.strokeStyle = "#FFFFFF"
//         ctx.lineWidth = 2;
//         ctx.strokeRect(Math.floor(16 * (1 + a_h) - 1), 16 * 18 - 1, 18, 18);

//         ctx.strokeRect(Math.floor(16 * (1 + a_s) - 1), 16 * (1 + a_b) - 1, 18, 18);
        
//         if(!hold_h && !pmouse.pressed && mouse.pressed && rx > 16 && rx < 16 * 17 && ry > 16 * 18 && ry < 16 * 19) {
//             hold_h = true;
//         }

//         if(!hold_sb && !pmouse.pressed && mouse.pressed && rx > 16 && rx < 16 * 17 && ry > 16 && ry < 16 * 17) {
//             hold_sb = true;
//         }

//         if(hold_h) {
//             h = Math.floor((rx - 16) / 16);
//             if(h < 0) h = 0;
//             if(h > 16 - 1) h = 16 - 1;
//             if(pmouse.pressed && !mouse.pressed) {
//                 hold_h = false;

//                 let [__h, __s, __b] = hsb2hsl(hue[h][3], Math.floor(100 * s / 15), Math.floor(100 * b / 15));
//                 let [_r, _g, _b] = hsl2rgb(__h, __s, __b);
//                 picked.r = _r;
//                 picked.g = _g;
//                 picked.b = _b;
//             }
//         }

//         if(hold_sb) {
//             s = Math.floor((rx - 16) / 16);
//             b = Math.floor((ry - 16) / 16);
//             if(s < 0) s = 0;
//             if(s > 16 - 1) s = 16 - 1;
//             if(b < 0) b = 0;
//             if(b > 16 - 1) b = 16 - 1;
//             if(pmouse.pressed && !mouse.pressed) {
//                 hold_sb = false;

//                 let [__h, __s, __b] = hsb2hsl(hue[h][3], Math.floor(100 * s / 15), Math.floor(100 * b / 15));
//                 let [_r, _g, _b] = hsl2rgb(__h, __s, __b);
//                 picked.r = _r;
//                 picked.g = _g;
//                 picked.b = _b;
//             }
//         }

//         a_h += (h - a_h) * 0.2;
//         if(Math.abs(h - a_h) < 0.1) {
//             a_h = h;
//         }

//         a_s += (s - a_s) * 0.2;
//         if(Math.abs(s - a_s) < 0.1) {
//             a_s = s;
//         }

//         a_b += (b - a_b) * 0.2;
//         if(Math.abs(b - a_b) < 0.1) {
//             a_b = b;
//         }
        
//     }, 10);

// }

// function board() {
//     let ox = 0, oy = 0;
//     let scale = 8;

//     const main = document.getElementsByClassName("main")[0];
//     //main.style.transform = "scale(16, 16)";

//     const canvas = document.getElementById("canvas");
//     canvas.style.transform = "scale(" + scale + "," + scale + ")";

//     //canvas.width = window.innerWidth;
//     //canvas.height = window.innerHeight;

//     canvas.width = 1000;
//     canvas.height = 1000;
//     canvas.style.position = "relative";
//     canvas.style.left = "calc(50vw - " + (canvas.width * 0.5) + "px + " + (ox * scale) + "px)";
//     canvas.style.top = "calc(50vh - " + (canvas.height * 0.5) + "px + " + (oy * scale) + "px)";

//     let pmouse = { x: 0, y: 0 };
//     let mouse = { x: 0, y: 0, btn: 0, pressed: false };
//     let amouse = { x: 0, y: 0, btn: 0, pressed: false };

//     let show = 0;

//     document.addEventListener('mousemove', (event) => {
//         amouse.x = event.clientX;
//         amouse.y = event.clientY;
//         amouse.btn = event.buttons;
//     });
//     document.addEventListener('mouseup', (event) => {
//         amouse.pressed = false;
//         amouse.btn = 0;
//     });
//     document.addEventListener('mousedown', (event) => {
//         amouse.pressed = true;
//         amouse.btn = event.buttons;
//     });
//     document.addEventListener("wheel", (event) => {
//         let delta = Math.sign(event.deltaY);
//         scale -= (scale * delta) * 0.1;
//         if(scale < 0.5) {
//             scale = 0.5;
//         } else if(scale > 64.0) {
//             scale = 64.0;
//         } else {
//             scale = Math.floor(scale * 100) / 100;
//         }
//     });
//     document.addEventListener('keydown', (event) => {
//         if(event.key == "Shift") {
//             show = 1;
//         }
//     });
//     document.addEventListener('keyup', (event) => {
//         if(event.key == "Shift") {
//             show = 0;
//         }
//     });

//     const ctx = canvas.getContext("2d");
//     ctx.imageSmoothingEnabled = false;

//     let w = 1000;
//     let h = 1000;
//     // const pixels = new Uint8ClampedArray(w * h * 4);
//     // for(let k = 0; k<w * h; k++) {
//     //     let r = Math.floor(Math.random() * 256);
//     //     let g = Math.floor(Math.random() * 256);
//     //     let b = Math.floor(Math.random() * 256);
//     //     pixels[k * 4 + 0] = r;
//     //     pixels[k * 4 + 1] = g;
//     //     pixels[k * 4 + 2] = b;
//     //     pixels[k * 4 + 3] = 255;
//     // }
    
//     // let img = new ImageData(pixels, w, h);

//     let a_scale = scale;
//     const positiontext = document.getElementById("position-text");

//     setInterval(() => {
//         if(img == null) return;
        
//         ctx.putImageData(img, 0, 0);

//         pmouse.x = mouse.x;
//         pmouse.y = mouse.y;
//         mouse = { ...amouse };
        
//         if(mouse.pressed && mouse.btn === 2) {
//             ox += (mouse.x - pmouse.x) / a_scale;
//             oy += (mouse.y - pmouse.y) / a_carscale;
//         }

//         a_scale += (scale - a_scale) * 0.2;
//         if(Math.abs(a_scale - scale) < scale * 0.02) {
//             a_scale = scale;
//         }

//         canvas.style.transform = "scale(" + a_scale + "," + a_scale + ")";
//         canvas.style.left = "calc(50vw - " + (canvas.width * 0.5) + "px + " + (ox * a_scale) + "px)";
//         canvas.style.top = "calc(50vh - " + (canvas.height * 0.5) + "px + " + (oy * a_scale) + "px)";

        
//         let bound = main.getBoundingClientRect();
//         // let cleft = bound.width * 0.5 - canvas.width * 0.5 * a_scale + (ox * a_scale);
//         // let ctop = bound.height * 0.5 - canvas.height * 0.5 * a_scale + (oy * a_scale);

//         let x = Math.floor((mouse.x - bound.width * 0.5) / a_scale - (ox - 500));
//         let y = Math.floor((mouse.y - bound.height * 0.5) / a_scale - (oy - 500));

//         if(x >= 0 && y >= 0 && x < 1000 && y < 1000) {
//             positiontext.innerHTML = "X" + x + " Y" + y;
//         } else {
//             positiontext.innerHTML = "";
//         }
        
//         // position.style.left = "calc(" + cleft + "px + " + (Math.floor((mouse.x - cleft) / a_scale) * a_scale + a_scale * 0.5 - 16) + "px)";
//         // position.style.top = "calc(" + ctop + "px + " + (Math.floor((mouse.y - ctop) / a_scale) * a_scale + a_scale * 0.5 - 16) + "px)";
//         // //position.style.transform = "scale(" + (a_scale / 32) + "," + (a_scale / 32) + ")";
//         // position.style.scale = a_scale / 32;

//         ctx.fillStyle = "#000000C0";
//         // ctx.fillRect(0, y, x, 1);
//         // ctx.fillRect(x + 1, y, 1000 - x - 1, 1);
        
//         // ctx.fillRect(x, 0, 1, y);
//         // ctx.fillRect(x, y + 1, 1, 1000 - y - 1);

//         // ctx.fillRect(x - 1, y - 1, 3, 1);
//         // ctx.fillRect(x - 1, y + 1, 3, 1);

//         if(mouse.pressed && mouse.btn === 1 && !account.logged) {
//             navToggle();
//             pageToggle("login-page");
//             return;
//         }

//         if(mouse.pressed
//         && mouse.btn === 1
//         && (account.last + cooldown() * 1000 <= Date.now())
//         && x >= 0
//         && x < 1000
//         && y >= 0
//         && y < 1000
//         // color picker
//         && (!show || mouse.x < bound.width - 16 * 18 || mouse.y < bound.height - 16 * 20)
//         // menu button
//         && (mouse.x < bound.width - 16 * 4 || mouse.y > 16 * 4)
//         && !document.querySelector('body').classList.contains('nav-active')
//         ) {

//             let k = 4 * (y * 1000 + x);
//             pixels[k + 0] = picked.r;
//             pixels[k + 1] = picked.g;
//             pixels[k + 2] = picked.b;

//             account.last = Date.now();
//             socket.send(JSON.stringify({
//                 'SetPixel': {
//                     'color': [picked.r, picked.g, picked.b],
//                     'idx': (y * 1000 + x)
//                 }
//             }));
//         }

//     }, 10);
// }

// 0-X 0-100, 0-100 => 0-X 0-100 0-100
function hsb2hsl(hue, saturation, brightness) {
    saturation /= 100;
    brightness /= 100;
    let ll = (2 - saturation) * brightness;
    let ss = saturation * brightness;
    ss /= ll <= 1 ? (ll !== 0 ? ll : 1) : (2 - ll !== 0 ? 2 - ll : 1);
    ll /= 2;
    return [hue, Math.round(ss * 100), Math.round(ll * 100)];
};

// 0-360 0-100, 0-100 => 0-255 0-255 0-255
function hsl2rgb(hue, saturation, lightness) {
    saturation /= 100;
    lightness /= 100;
    const C = (1 - Math.abs(2 * lightness - 1)) * saturation;
    const X = C * (1 - Math.abs(hue / 60 % 2 - 1));
    const m = lightness - C / 2;
    let [R, G, B] =
        (0 <= hue && hue < 60 && [C, X, 0]) ||
        (60 <= hue && hue < 120 && [X, C, 0]) ||
        (120 <= hue && hue < 180 && [0, C, X]) ||
        (180 <= hue && hue < 240 && [0, X, C]) ||
        (240 <= hue && hue < 300 && [X, 0, C]) ||
        (300 <= hue && hue < 360 && [C, 0, X]);
    [R, G, B] = [(R + m) * 255, (G + m) * 255, (B + m) * 255];
    return [Math.round(R), Math.round(G), Math.round(B)];
};