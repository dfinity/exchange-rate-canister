use serde::Serialize;
use tera::{Context, Tera};

/// The entrypoint init.sh to be generated. This script generates the certificates,
/// updates the CA certs, and adds the domains to the /etc/hosts file.
const INIT_SH: &str = r#"
#!/usr/bin/env bash

if [ ! -f /certs/minica.pem ]; then
    mkdir /certs
    cd /certs

    # Generate the certs for each known domain.
    {% for host, locations in items %}minica --domains "{{ host }}"
    {% endfor %}

    # Setup nginx's certs directory.
    mkdir -p /etc/nginx/certs

    # Add minica to ca-certificates
    chmod 0644 minica.pem
    ls -la
    cp minica.pem /usr/local/share/ca-certificates/minica.crt
    update-ca-certificates

    # Move certs to appropriate directory
    {% for host, locations in items %}mv /certs/{{ host }} /etc/nginx/certs/{{ host }}
    {% endfor %}

fi

# Map domain to localhost
{% for host, locations in items %}echo "127.0.0.1 {{ host }}" >> /etc/hosts
{% endfor %}
cat /etc/hosts
"#;

/// The template to generate the nginx.conf from the provided responses.
const NGINX_SERVER_CONF: &str = r#"
lua_package_path "/etc/nginx/?.lua;;";
{% for host, config in items %}
server {
    listen       443 ssl;
    listen  [::]:443;
    server_name  {{ host }};
    ssl_certificate /etc/nginx/certs/{{ host }}/cert.pem;
    ssl_certificate_key /etc/nginx/certs/{{ host }}/key.pem;

    root '/srv';
    error_log /var/log/nginx/{{ config.name }}.error.log debug;

    location / {
        set_by_lua_block $json_filename {
            local uri = ngx.var.uri
            local args, err = ngx.req.get_uri_args()
            if err == "truncated" then
                return "404.json"
            end
            local filename = require("router").route("{{ config.name }}", uri, args, "json")
            ngx.log(ngx.ERR, filename)
            return filename
        }

        set_by_lua_block $xml_filename {
            local uri = ngx.var.uri
            local args, err = ngx.req.get_uri_args()
            if err == "truncated" then
                return "404.json"
            end
            local filename = require("router").route("{{ config.name }}", uri, args, "xml")
            ngx.log(ngx.ERR, filename)
            return filename
        }

        try_files $json_filename $xml_filename =404;
    }

}
{% endfor %}
"#;

/// The choice of template that should be rendered.
pub enum Template {
    InitSh,
    NginxConf,
}

/// This function handles the actual rendering of the templates to a string.
pub fn render<T>(template: Template, items: &T) -> Result<String, tera::Error>
where
    T: Serialize,
{
    let template = match template {
        Template::InitSh => INIT_SH,
        Template::NginxConf => NGINX_SERVER_CONF,
    };

    let mut tera = Tera::default();
    let mut context = Context::new();
    context.insert("items", items);
    tera.render_str(template, &context)
}
