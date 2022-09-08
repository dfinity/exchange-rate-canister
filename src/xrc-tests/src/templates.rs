use serde::Serialize;
use tera::{Context, Tera};

/// The entrypoint init.sh to be generated. This script generates the certificates,
/// updates the CA certs, and adds the domains to the /etc/hosts file.
pub const INIT_SH: &str = r#"
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

/// The template so the nginx.conf can be generated from the provided responses.
pub const NGINX_SERVER_CONF: &str = r#"
{% for host, config in items %}
server {
    listen       443 ssl;
    listen  [::]:443;
    server_name  {{ host }};
    ssl_certificate /etc/nginx/certs/{{ host }}/cert.pem;
    ssl_certificate_key /etc/nginx/certs/{{ host }}/key.pem;

    {% for location in config.locations %}
    location {{ location.path }} {
        {% if location.status_code == 200 %}
        alias /srv/{{ config.name }}.json;
        {% else %}
        return {{ location.status_code }}
        {% endif %}
    }
    {% endfor %}
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
