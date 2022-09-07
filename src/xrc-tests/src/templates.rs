use serde::Serialize;
use tera::{Context, Tera};

pub const INIT_SH: &str = r#"
#!/usr/bin/env bash

if [ ! -f /certs/minica.pem ]; then
    mkdir /certs
    cd /certs

    {% for host, locations in items %}minica --domains "{{ host }}"
    {% endfor %}

    mkdir -p /etc/nginx/certs
    chmod 0644 minica.pem

    ls -la
    cp minica.pem /usr/local/share/ca-certificates/minica.crt
    update-ca-certificates

    {% for host, locations in items %}mv /certs/{{ host }} /etc/nginx/certs/{{ host }}
    {% endfor %}

fi

{% for host, locations in items %}echo "127.0.0.1 {{ host }}" >> /etc/hosts
{% endfor %}
cat /etc/hosts
"#;

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

pub enum Template {
    InitSh,
    NginxConf,
}

pub fn render<T>(template: Template, items: &T) -> Result<String, String>
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
        .map_err(|err| format!("Failed to render template! {:?}", err))
}
