use serde::Serialize;
use tera::{Context, Tera};

use url::Url;
use xrc::EXCHANGES;

pub const CERTS_AND_KEYS_SH: &str = r#"
#!/usr/bin/env bash

if [ ! -f /certs/minica.pem ]; then
    mkdir /certs
    cd /certs

    {% for item in items %}minica --domains "{{ item }}"
    {% endfor %}

    mkdir -p /etc/nginx/certs
    chmod 0644 minica.pem

    ls -la
    cp minica.pem /usr/local/share/ca-certificates/minica.crt
    update-ca-certificates

    {% for item in items %}mv /certs/{{ item }} /etc/nginx/certs/{{ item }}
    {% endfor %}

fi

{% for item in items %}echo "127.0.0.1 {{ item }}" >> /etc/hosts
{% endfor %}
cat /etc/hosts
"#;

pub const NGINX_SERVER_CONF: &str = r#"
{% for item in items %}
server {
    listen       443 ssl;
    listen  [::]:443;
    server_name  {{ item.host }};
    ssl_certificate /etc/nginx/certs/{{ item.host }}/cert.pem;
    ssl_certificate_key /etc/nginx/certs/{{ item.host }}/key.pem;

    location {{ item.path }} {
        {% if item.status_code == 200 %}
        alias /srv/{{ item.name }}.json;
        {% else %}
        return {{ item.status_code }}
        {% endif %}
    }

}
{% endfor %}
"#;

pub fn render<T>(template: &str, items: &T) -> Result<String, String>
where
    T: Serialize,
{
    let mut tera = Tera::default();
    let mut context = Context::new();
    context.insert("items", items);
    tera.render_str(template, &context)
        .map_err(|err| format!("Failed to render proposals document! {:?}", err))
}

pub fn render_certs_and_keys_sh() -> Result<String, String> {
    let hosts = EXCHANGES
        .iter()
        .map(|e| {
            let url = Url::parse(&e.get_url("", "", 0)).expect("failed to parse");
            url.host()
                .expect("exchange url should have a host")
                .to_string()
        })
        .collect::<Vec<_>>();
    render(CERTS_AND_KEYS_SH, &hosts)
}
