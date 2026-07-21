use std::{collections::BTreeSet, fs, path::Path};

use std::collections::BTreeMap;

use crate::{
    blueprint::{
        Blueprint, CanonicalType, Column, ConnectionConfig, DatabaseProvider, EntityConfig,
        EntityFieldConfig, Table,
    },
    error::AppError,
};

use super::{
    ownership::{hash_bytes, validate_relative_path},
    GeneratedFile, Ownership,
};

#[derive(Clone)]
struct EntitySpec {
    key: String,
    slug: String,
    label: String,
    model: String,
    delegate: String,
    table: String,
    fields: Vec<FieldSpec>,
    primary: FieldSpec,
}

#[derive(Clone)]
struct FieldSpec {
    key: String,
    prisma: String,
    column: String,
    kind: CanonicalType,
    nullable: bool,
    primary: bool,
    show_list: bool,
    show_view: bool,
    show_form: bool,
    required: bool,
    control: String,
    validations: Vec<(String, Option<String>, Option<String>)>,
}

pub(super) fn render_project(
    blueprint: &Blueprint,
    project_root: &Path,
) -> Result<Vec<GeneratedFile>, AppError> {
    let entities = entity_specs(blueprint)?;
    if entities.is_empty() {
        return Err(AppError::Validation);
    }
    let auth_enabled = blueprint.auth.is_some();
    let mut files = static_files(blueprint, &entities)?;
    files.push(generated("prisma/schema.prisma", prisma_schema(blueprint)?));
    for entity in &entities {
        files.extend(render_entity(entity, auth_enabled));
    }
    files.extend(security_files(blueprint, &entities)?);
    if let Some(entity) = entities.iter().find(|entity| {
        entity.fields.iter().any(|field| {
            field.show_form
                && field.required
                && !field.nullable
                && matches!(field.kind, CanonicalType::Text | CanonicalType::Unknown)
        }) && entity
            .fields
            .iter()
            .filter(|field| field.show_form)
            .all(|field| field.validations.is_empty())
            && entity.fields.iter().all(|field| {
                field.nullable
                    || field.show_form
                    || (field.primary && field.kind == CanonicalType::Integer)
            })
    }) {
        files.push(generated(
            &format!("src/features/{}/crud.integration.test.ts", entity.slug),
            entity_crud_test(entity),
        ));
    }
    files.extend(extension_files(blueprint, project_root)?);
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let mut paths = BTreeSet::new();
    if files.iter().any(|file| !paths.insert(file.path.clone())) {
        return Err(AppError::Conflict);
    }
    Ok(files)
}

fn static_files(
    blueprint: &Blueprint,
    entities: &[EntitySpec],
) -> Result<Vec<GeneratedFile>, AppError> {
    let project_name = package_name(&blueprint.project_name);
    let database_url = match (
        &blueprint.databases.main.provider,
        &blueprint.databases.main.connection,
    ) {
        (DatabaseProvider::Sqlite, ConnectionConfig::Sqlite { path }) => {
            format!("file:{}", path.replace('\\', "/"))
        }
        (DatabaseProvider::Postgresql, ConnectionConfig::Server { .. })
        | (DatabaseProvider::Mysql, ConnectionConfig::Server { .. }) => String::new(),
        _ => return Err(AppError::Validation),
    };
    let env_example = match blueprint.databases.main.provider {
        DatabaseProvider::Sqlite => "DATABASE_URL=\"file:./prisma/dev.sqlite\"\n",
        DatabaseProvider::Postgresql => {
            "DATABASE_URL=\"postgresql://USER:PASSWORD@HOST:5432/DATABASE?schema=public\"\n"
        }
        DatabaseProvider::Mysql => "DATABASE_URL=\"mysql://USER:PASSWORD@HOST:3306/DATABASE\"\n",
    };
    let entity_cards = entities
        .iter()
        .map(|entity| format!(r#"        <Link className="group" href="/{slug}"><Card className="h-full border-border/70 shadow-none transition-all duration-200 hover:-translate-y-0.5 hover:border-foreground/20 hover:shadow-md"><CardHeader className="p-5"><div className="flex items-start justify-between gap-4"><div className="grid size-9 place-items-center rounded-lg bg-muted"><Database className="size-4 text-muted-foreground" /></div><ArrowUpRight className="size-4 text-muted-foreground transition-transform group-hover:-translate-y-0.5 group-hover:translate-x-0.5" /></div><CardTitle className="mt-5 text-base">{label}</CardTitle><CardDescription>Browse and manage records.</CardDescription></CardHeader></Card></Link>"#, slug = entity.slug, label = escape_html(&entity.label)))
        .collect::<Vec<_>>()
        .join("\n");
    let package = format!(
        r#"{{
  "name": "{project_name}",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {{
    "dev": "next dev",
    "build": "prisma generate && next build",
    "start": "next start",
    "typecheck": "prisma generate && tsc --noEmit",
    "lint": "eslint .",
    "test": "vitest run --configLoader runner",
    "prisma:generate": "prisma generate",
    "prisma:push": "prisma db push"
  }},
  "dependencies": {{
    "@hookform/resolvers": "5.2.2",
    "@prisma/adapter-better-sqlite3": "7.8.0",
    "@prisma/client": "7.8.0",
    "@tanstack/react-table": "8.21.3",
    "bcryptjs": "3.0.3",
    "better-sqlite3": "12.4.1",
    "class-variance-authority": "0.7.1",
    "clsx": "2.1.1",
    "dotenv": "17.2.3",
    "lucide-react": "0.468.0",
    "next": "16.2.10",
    "next-auth": "4.24.14",
    "react": "19.2.0",
    "react-dom": "19.2.0",
    "react-hook-form": "7.81.0",
    "tailwind-merge": "2.5.5",
    "zod": "4.4.1"
  }},
  "devDependencies": {{
    "@tailwindcss/postcss": "4.3.1",
    "@types/better-sqlite3": "7.6.13",
    "@types/node": "24.10.1",
    "@types/react": "19.2.2",
    "@types/react-dom": "19.2.2",
    "eslint": "9.39.1",
    "eslint-config-next": "16.2.10",
    "prisma": "7.8.0",
    "tailwindcss": "4.3.1",
    "typescript": "6.0.3",
    "vitest": "4.1.10"
  }},
  "engines": {{ "node": ">=22.13.0" }}
}}
"#
    );
    Ok(vec![
        generated("package.json", package),
        user_optional("package-lock.json"),
        generated("next.config.ts", "import type { NextConfig } from \"next\";\n\nconst config: NextConfig = { reactStrictMode: true, turbopack: { root: process.cwd() } };\nexport default config;\n"),
        generated("postcss.config.mjs", "const config = { plugins: { \"@tailwindcss/postcss\": {} } };\nexport default config;\n"),
        generated("eslint.config.mjs", "import { defineConfig, globalIgnores } from \"eslint/config\";\nimport nextVitals from \"eslint-config-next/core-web-vitals\";\n\nexport default defineConfig([...nextVitals, { rules: { \"@next/next/no-html-link-for-pages\": \"off\" } }, globalIgnores([\".next/**\", \"src/generated/**\"])]);\n"),
        generated("tsconfig.json", TSCONFIG),
        generated("vitest.config.ts", "import { resolve } from \"node:path\";\nimport { fileURLToPath } from \"node:url\";\nimport { defineConfig } from \"vitest/config\";\n\nconst root = fileURLToPath(new URL(\".\", import.meta.url));\nexport default defineConfig({ resolve: { alias: { \"@\": resolve(root, \"src\") } }, test: { environment: \"node\", include: [\"src/**/*.test.ts\"] } });\n"),
        generated("prisma.config.ts", PRISMA_CONFIG),
        generated("components.json", SHADCN_COMPONENTS_JSON),
        user(".env", format!("DATABASE_URL={}\n", dotenv_quote(&database_url))),
        generated(".env.local", format!("NEXTAUTH_URL=http://localhost:3000\nNEXTAUTH_SECRET={}\n", hash_bytes(blueprint.project_id.as_bytes()))),
        generated(".env.example", format!("{env_example}NEXTAUTH_URL=http://localhost:3000\nNEXTAUTH_SECRET=replace-with-a-long-random-value\n")),
        generated(".gitignore", "node_modules/\n.next/\n.env\n.env.local\nsrc/generated/prisma/\n*.log\n"),
        generated("src/lib/prisma.ts", prisma_client(blueprint.databases.main.provider)),
        generated("src/lib/utils.ts", SHADCN_UTILS),
        generated("src/components/ui/button.tsx", SHADCN_BUTTON),
        generated("src/components/ui/card.tsx", SHADCN_CARD),
        generated("src/components/ui/input.tsx", SHADCN_INPUT),
        generated("src/components/ui/label.tsx", SHADCN_LABEL),
        generated("src/components/ui/table.tsx", SHADCN_TABLE),
        generated("src/components/ui/textarea.tsx", SHADCN_TEXTAREA),
        generated("src/components/app-sidebar.tsx", app_sidebar(&blueprint.project_name, entities)),
        generated("src/lib/query-contract.ts", QUERY_CONTRACT),
        generated("src/lib/query-contract.test.ts", QUERY_CONTRACT_TEST),
        generated("src/extensions/types.ts", EXTENSION_TYPES),
        generated("src/extensions/registry.ts", extension_registry(blueprint)),
        generated("src/app/globals.css", GLOBAL_CSS),
        generated("src/app/layout.tsx", LAYOUT.replace("{{PROJECT_NAME}}", &escape_tsx(&blueprint.project_name))),
        generated("src/app/(dashboard)/layout.tsx", (if blueprint.auth.is_some() { DASHBOARD_LAYOUT_AUTH } else { DASHBOARD_LAYOUT }).replace("{{PROJECT_NAME}}", &escape_tsx(&blueprint.project_name))),
        generated("src/app/(dashboard)/page.tsx", DASHBOARD_PAGE.replace("{{ENTITY_CARDS}}", &entity_cards)),
    ])
}

fn app_sidebar(project_name: &str, entities: &[EntitySpec]) -> String {
    let items = entities
        .iter()
        .map(|entity| {
            format!(
                "  {{ href: {}, label: {} }},",
                js_string(&format!("/{}", entity.slug)),
                js_string(&entity.label)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#""use client";
import Link from "next/link";
import {{ usePathname }} from "next/navigation";
import {{ Boxes, Database, LayoutDashboard }} from "lucide-react";
import {{ cn }} from "@/lib/utils";

const navigation = [
{items}
] as const;

function isActive(pathname: string, href: string) {{ return href === "/" ? pathname === "/" : pathname === href || pathname.startsWith(`${{href}}/`); }}

export function AppSidebar() {{
  const pathname = usePathname();
  const itemClass = (active: boolean) => cn("flex items-center gap-2.5 rounded-lg px-3 py-2 text-sm font-medium transition-colors", active ? "bg-primary text-primary-foreground shadow-sm hover:bg-primary/90" : "text-muted-foreground hover:bg-muted hover:text-foreground");
  return <aside className="hidden border-r bg-background md:fixed md:inset-y-0 md:flex md:w-60 md:flex-col"><div className="flex h-16 items-center gap-2 border-b px-5"><span className="grid size-7 place-items-center rounded-md bg-primary text-primary-foreground"><Boxes className="size-4" /></span><Link className="truncate text-sm font-semibold" href="/">{project_name}</Link></div><nav className="grid gap-1 p-3"><Link className={{itemClass(isActive(pathname, "/"))}} href="/"><LayoutDashboard className="size-4" />Overview</Link>{{navigation.map((item) => <Link className={{itemClass(isActive(pathname, item.href))}} href={{item.href}} key={{item.href}}><Database className="size-4" />{{item.label}}</Link>)}}</nav><div className="mt-auto border-t px-5 py-4 text-xs text-muted-foreground">Generated by Emanduite</div></aside>;
}}
"#,
        items = items,
        project_name = escape_tsx(project_name)
    )
}

fn security_files(
    blueprint: &Blueprint,
    entities: &[EntitySpec],
) -> Result<Vec<GeneratedFile>, AppError> {
    let resources = blueprint
        .resources
        .values()
        .map(|resource| (resource.id.as_str(), resource.key.as_str()))
        .collect::<BTreeMap<_, _>>();
    let roles = blueprint
        .roles
        .iter()
        .map(|(key, role)| {
            let permissions = role
                .permissions
                .iter()
                .filter_map(|(resource_id, actions)| {
                    resources.get(resource_id.as_str()).map(|resource| {
                        format!(
                            "{}: [{}]",
                            js_string(resource),
                            actions
                                .iter()
                                .map(|action| js_string(action))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}: {{ {} }}", js_string(key), permissions)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut files = vec![
        generated("src/lib/access-policy.ts", format!("export const rolePermissions = {{ {roles} }} as const;\nexport type PermissionAction = \"read\" | \"create\" | \"update\" | \"delete\";\nexport function hasPermission(roleKey: string | undefined, resource: string, action: PermissionAction) {{ return Boolean(roleKey && rolePermissions[roleKey as keyof typeof rolePermissions]?.[resource as never]?.includes(action as never)); }}\n")),
        generated("src/lib/hooks.ts", HOOK_RUNTIME),
    ];
    if let Some(auth) = &blueprint.auth {
        files.extend(auth_files(blueprint, entities, auth)?);
    }
    Ok(files)
}

fn auth_files(
    blueprint: &Blueprint,
    entities: &[EntitySpec],
    auth: &crate::blueprint::AuthConfig,
) -> Result<Vec<GeneratedFile>, AppError> {
    let config = blueprint
        .entities
        .values()
        .find(|entity| entity.id == auth.user_entity_id)
        .ok_or(AppError::Validation)?;
    let user_key = blueprint
        .entities
        .iter()
        .find(|(_, entity)| entity.id == auth.user_entity_id)
        .map(|(key, _)| key.clone())
        .ok_or(AppError::Validation)?;
    let user = entities
        .iter()
        .find(|entity| entity.key == user_key)
        .ok_or(AppError::Validation)?;
    let field = |id: &str| -> Result<String, AppError> {
        let key = config
            .fields
            .iter()
            .find(|(_, field)| field.id == id)
            .map(|(key, _)| key)
            .ok_or(AppError::Validation)?;
        user.fields
            .iter()
            .find(|field| &field.key == key)
            .map(|field| field.prisma.clone())
            .ok_or(AppError::Validation)
    };
    let identifier = field(&auth.identifier_field_id)?;
    let password = field(&auth.password_field_id)?;
    let external = field(&auth.external_id_field_id)?;
    let registration = matches!(
        auth.registration_policy,
        crate::blueprint::RegistrationPolicy::Open
    );
    let auth_secret = hash_bytes(blueprint.project_id.as_bytes());
    let auth_source = format!(
        r#"import NextAuth, {{ type NextAuthOptions }} from "next-auth";
import CredentialsProvider from "next-auth/providers/credentials";
import {{ compare }} from "bcryptjs";
import {{ prisma }} from "@/lib/prisma";

export const authOptions: NextAuthOptions = {{
  secret: process.env.NEXTAUTH_SECRET ?? "{auth_secret}",
  session: {{ strategy: "jwt" }},
  pages: {{ signIn: "/login" }},
  providers: [CredentialsProvider({{
    name: "Credentials",
    credentials: {{ identifier: {{ label: "Identifier", type: "text" }}, password: {{ label: "Password", type: "password" }} }},
    async authorize(credentials) {{
  const identifier = credentials?.identifier?.trim(); const password = credentials?.password;
  if (!identifier || !password) return null;
  const user = await prisma.{delegate}.findFirst({{ where: {{ {identifier}: identifier }} }});
  const candidate = user as Record<string, unknown> | null;
  if (!candidate || !(await compare(password, String(candidate.{password} ?? "")))) return null;
  const roleKey = Number(candidate.roleId ?? 0) === 1 ? "superadmin" : "";
  if (!roleKey) return null;
  return {{ id: String(candidate.{external}), name: String(candidate.{identifier}), roleKey }} as never;
    }}
  }})],
  callbacks: {{
    async jwt({{ token, user }}) {{ if (user) token.roleKey = (user as {{ roleKey?: string }}).roleKey; return token; }},
    async session({{ session, token }}) {{ (session.user as {{ roleKey?: string }} | undefined)!.roleKey = String(token.roleKey ?? ""); return session; }}
  }}
}};
export default NextAuth(authOptions);
"#,
        delegate = user.delegate,
        identifier = identifier,
        password = password,
        external = external,
        auth_secret = auth_secret
    );
    let access = r#"import { getServerSession } from "next-auth";
import { authOptions } from "@/auth";
import { hasPermission, type PermissionAction } from "./access-policy";

export async function requirePermission(resource: string, action: PermissionAction) {
  const session = await getServerSession(authOptions); const roleKey = (session?.user as { roleKey?: string } | undefined)?.roleKey;
  if (!hasPermission(roleKey, resource, action)) throw new Error("FORBIDDEN");
  return session;
}
"#;
    let proxy = format!(
        r#"import {{ getToken }} from "next-auth/jwt";
import {{ NextResponse, type NextRequest }} from "next/server";

const authSecret = process.env.NEXTAUTH_SECRET ?? "{auth_secret}";
export async function proxy(request: NextRequest) {{
  try {{
    const token = await getToken({{ req: request, secret: authSecret }});
    if (token) return NextResponse.next();
  }} catch {{
    // Fail closed when session verification is unavailable.
  }}
  return NextResponse.redirect(new URL("/login", request.url));
}}
export const config = {{ matcher: ["/((?!api/auth|login|register|_next|favicon.ico).*)"] }};
"#,
        auth_secret = auth_secret
    );
    let login = r#""use client";
import { signIn } from "next-auth/react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
export default function LoginPage() { const [error,setError] = useState(""); return <main className="grid min-h-screen place-items-center bg-muted/40 p-4"><Card className="w-full max-w-sm"><CardHeader><CardTitle>Sign in</CardTitle><CardDescription>Use the account provisioned for this workspace.</CardDescription></CardHeader><CardContent><form className="grid gap-5" action={async (form) => { const result = await signIn("credentials", { identifier: String(form.get("identifier") ?? ""), password: String(form.get("password") ?? ""), redirect: true, callbackUrl: "/" }); if (result?.error) setError("Invalid credentials"); }}><div className="grid gap-2"><Label htmlFor="identifier">Identifier</Label><Input id="identifier" name="identifier" required /></div><div className="grid gap-2"><Label htmlFor="password">Password</Label><Input id="password" name="password" type="password" required /></div>{error && <p className="text-sm text-destructive" role="alert">{error}</p>}<Button type="submit">Sign in</Button></form></CardContent></Card></main>; }
"#;
    let mut files = vec![
        generated("src/auth.ts", auth_source),
        generated("src/lib/access.ts", access),
        generated(
            "src/app/api/auth/[...nextauth]/route.ts",
            "import auth from \"@/auth\";\nexport { auth as GET, auth as POST };\n",
        ),
        generated("proxy.ts", proxy),
        generated("src/app/login/page.tsx", login),
    ];
    if registration {
        files.push(generated("src/app/register/page.tsx", "export default function RegisterPage() { return <main className=\"auth-page\"><p>Registration is enabled; implement the approved provisioning flow before public use.</p></main>; }\n"));
    }
    Ok(files)
}

fn entity_specs(blueprint: &Blueprint) -> Result<Vec<EntitySpec>, AppError> {
    let mut values = Vec::new();
    let mut slugs = BTreeSet::new();
    let mut models = BTreeSet::new();
    for (key, entity) in &blueprint.entities {
        if entity.database_id != blueprint.databases.main.id {
            return Err(AppError::Validation);
        }
        let table = blueprint
            .databases
            .main
            .tables
            .iter()
            .find(|table| table.id == entity.table_id)
            .ok_or(AppError::Validation)?;
        let slug = slug(key)?;
        let model = pascal_identifier(&table.name);
        if !slugs.insert(slug.clone()) || !models.insert(model.clone()) {
            return Err(AppError::Conflict);
        }
        let fields = field_specs(table, entity)?;
        let primary_fields: Vec<_> = fields
            .iter()
            .filter(|field| field.primary)
            .cloned()
            .collect();
        if primary_fields.len() != 1 {
            return Err(AppError::Validation);
        }
        values.push(EntitySpec {
            key: key.clone(),
            slug,
            label: entity.label.clone().unwrap_or_else(|| key.clone()),
            delegate: lower_first(&model),
            model,
            table: table.name.clone(),
            primary: primary_fields[0].clone(),
            fields,
        });
    }
    Ok(values)
}

fn field_specs(table: &Table, entity: &EntityConfig) -> Result<Vec<FieldSpec>, AppError> {
    let mut values = Vec::new();
    let mut names = BTreeSet::new();
    for column in &table.columns {
        let configured = entity
            .fields
            .iter()
            .find(|(_, field)| field.column_id == column.id);
        let (key, config) = configured.map_or_else(
            || (column.name.clone(), None),
            |(key, config)| (key.clone(), Some(config)),
        );
        let prisma = camel_identifier(&column.name);
        if !names.insert(prisma.clone()) {
            return Err(AppError::Conflict);
        }
        if config.is_some_and(|field| matches!(field.control.as_str(), "file" | "rich-text")) {
            return Err(AppError::CapabilityDenied);
        }
        if let Some(field) = config {
            for rule in &field.validation {
                match rule.kind.as_str() {
                    "minLength" | "maxLength" => {
                        let raw = rule.value.as_ref().map(ToString::to_string);
                        if !matches!(
                            column.canonical_type,
                            CanonicalType::Text | CanonicalType::Unknown
                        ) || json_number(raw.as_deref()).is_none()
                        {
                            return Err(AppError::Validation);
                        }
                    }
                    "pattern" => {
                        if !matches!(
                            column.canonical_type,
                            CanonicalType::Text | CanonicalType::Unknown
                        ) || rule
                            .value
                            .as_ref()
                            .and_then(|value| value.as_str())
                            .is_none()
                        {
                            return Err(AppError::Validation);
                        }
                    }
                    "email" => {
                        if !matches!(
                            column.canonical_type,
                            CanonicalType::Text | CanonicalType::Unknown
                        ) {
                            return Err(AppError::Validation);
                        }
                    }
                    _ => return Err(AppError::CapabilityDenied),
                }
            }
        }
        values.push(field_spec(key, prisma, column, config));
    }
    Ok(values)
}

fn field_spec(
    key: String,
    prisma: String,
    column: &Column,
    config: Option<&EntityFieldConfig>,
) -> FieldSpec {
    let auto = column.primary_key
        && column.canonical_type == CanonicalType::Integer
        && (column.native_type.to_ascii_uppercase().contains("INT")
            || column
                .default_value
                .as_deref()
                .is_some_and(|value| value.to_ascii_lowercase().contains("auto")));
    FieldSpec {
        key,
        prisma,
        column: column.name.clone(),
        kind: column.canonical_type,
        nullable: column.nullable,
        primary: column.primary_key,
        show_list: config.map_or(true, |field| field.show_in_list),
        show_view: config.map_or(true, |field| field.show_in_view),
        show_form: config.map_or(!column.primary_key || !auto, |field| field.show_in_form) && !auto,
        required: config.map_or(!column.nullable, |field| field.required),
        control: config.map_or_else(
            || default_control(column.canonical_type).into(),
            |field| field.control.clone(),
        ),
        validations: config.map_or_else(Vec::new, |field| {
            field
                .validation
                .iter()
                .map(|rule| {
                    (
                        rule.kind.clone(),
                        rule.value.as_ref().map(ToString::to_string),
                        rule.message.clone(),
                    )
                })
                .collect()
        }),
    }
}

fn prisma_schema(blueprint: &Blueprint) -> Result<String, AppError> {
    let provider = match blueprint.databases.main.provider {
        DatabaseProvider::Sqlite => "sqlite",
        DatabaseProvider::Postgresql => "postgresql",
        DatabaseProvider::Mysql => "mysql",
    };
    let mut output = format!("generator client {{\n  provider = \"prisma-client\"\n  output = \"../src/generated/prisma\"\n  moduleFormat = \"esm\"\n}}\n\ndatasource db {{\n  provider = \"{provider}\"\n}}\n");
    let mut models = BTreeSet::new();
    for table in &blueprint.databases.main.tables {
        if is_system_table(&table.name) {
            continue;
        }
        let model = pascal_identifier(&table.name);
        if !models.insert(model.clone()) {
            return Err(AppError::Conflict);
        }
        output.push_str(&format!("\nmodel {model} {{\n"));
        let mut names = std::collections::BTreeMap::new();
        let primary_columns: Vec<_> = table
            .columns
            .iter()
            .filter(|column| column.primary_key)
            .collect();
        for column in &table.columns {
            names.insert(column.name.clone(), camel_identifier(&column.name));
            let field = camel_identifier(&column.name);
            let kind = prisma_type(column.canonical_type);
            let optional = if column.nullable { "?" } else { "" };
            let id = if column.primary_key && primary_columns.len() == 1 {
                " @id"
            } else {
                ""
            };
            let default = prisma_default(column, primary_columns.len() == 1);
            output.push_str(&format!(
                "  {field} {kind}{optional}{id}{default} @map({})\n",
                prisma_string(&column.name)
            ));
        }
        if primary_columns.len() > 1 {
            let fields = primary_columns
                .iter()
                .map(|column| camel_identifier(&column.name))
                .collect::<Vec<_>>()
                .join(", ");
            output.push_str(&format!("  @@id([{fields}])\n"));
        } else if primary_columns.is_empty() {
            output.push_str("  @@ignore\n");
        }
        for index in &table.indexes {
            let columns: Vec<_> = index
                .columns
                .iter()
                .filter_map(|column| names.get(column))
                .cloned()
                .collect();
            if columns.len() == index.columns.len() && !columns.is_empty() {
                let directive = if index.unique { "@@unique" } else { "@@index" };
                output.push_str(&format!(
                    "  {directive}([{}], map: {})\n",
                    columns.join(", "),
                    prisma_string(&index.name)
                ));
            }
        }
        output.push_str(&format!("  @@map({})\n}}\n", prisma_string(&table.name)));
    }
    output.push_str(SYSTEM_MODELS);
    Ok(output)
}

fn is_system_table(name: &str) -> bool {
    matches!(
        name,
        "sys_roles" | "sys_auth_subjects" | "sys_resources" | "sys_permissions" | "sys_audit_logs"
    )
}

fn prisma_type(kind: CanonicalType) -> &'static str {
    match kind {
        CanonicalType::Integer => "Int",
        CanonicalType::Real | CanonicalType::Decimal => "Float",
        CanonicalType::Boolean => "Boolean",
        CanonicalType::Bytes => "Bytes",
        CanonicalType::Date | CanonicalType::DateTime => "DateTime",
        CanonicalType::Json => "Json",
        CanonicalType::Text | CanonicalType::Unknown => "String",
    }
}

fn prisma_default(column: &Column, single_primary: bool) -> String {
    if single_primary && column.primary_key && column.canonical_type == CanonicalType::Integer {
        return " @default(autoincrement())".into();
    }
    let Some(value) = column.default_value.as_deref().map(str::trim) else {
        return String::new();
    };
    if matches!(
        column.canonical_type,
        CanonicalType::Date | CanonicalType::DateTime
    ) && value.to_ascii_lowercase().contains("current_timestamp")
    {
        return " @default(now())".into();
    }
    String::new()
}

fn extension_files(
    blueprint: &Blueprint,
    project_root: &Path,
) -> Result<Vec<GeneratedFile>, AppError> {
    let mut output = Vec::new();
    for extension in blueprint.extensions.values() {
        validate_relative_path(&extension.path)?;
        let source = project_root.join("extensions").join(&extension.path);
        let content = if source.is_file() {
            fs::read_to_string(source)?
        } else {
            String::new()
        };
        output.push(user(
            &format!("src/extensions/user/{}", extension.path.replace('\\', "/")),
            content,
        ));
    }
    Ok(output)
}

fn extension_registry(blueprint: &Blueprint) -> String {
    let entries = blueprint
        .extensions
        .iter()
        .map(|(key, extension)| {
            format!(
                "  {}: {},",
                js_string(key),
                js_string(&format!("./user/{}", extension.path.replace('\\', "/")))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("import type {{ ExtensionRegistry }} from \"./types\";\n\nexport const extensionRegistry = {{\n{entries}\n}} as const satisfies ExtensionRegistry;\n")
}

fn generated(path: &str, content: impl Into<String>) -> GeneratedFile {
    GeneratedFile {
        path: path.into(),
        owner: Ownership::Generated,
        content: normalize(content.into()),
    }
}

fn user(path: &str, content: impl Into<String>) -> GeneratedFile {
    GeneratedFile {
        path: path.into(),
        owner: Ownership::User,
        content: normalize(content.into()),
    }
}

fn user_optional(path: &str) -> GeneratedFile {
    GeneratedFile {
        path: path.into(),
        owner: Ownership::User,
        content: String::new(),
    }
}

fn normalize(value: String) -> String {
    format!("{}\n", value.replace("\r\n", "\n").trim_end())
}

fn package_name(value: &str) -> String {
    let value = value.to_ascii_lowercase();
    let mut output = String::new();
    let mut dash = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character);
            dash = false;
        } else if !dash && !output.is_empty() {
            output.push('-');
            dash = true;
        }
    }
    let output = output.trim_matches('-');
    if output.is_empty() {
        "emanduite-generated".into()
    } else {
        output.into()
    }
}

fn slug(value: &str) -> Result<String, AppError> {
    let output = package_name(value);
    if output == "emanduite-generated" && !value.to_ascii_lowercase().contains("emanduite") {
        return Err(AppError::Validation);
    }
    Ok(output)
}

fn words(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
        .collect()
}

fn pascal_identifier(value: &str) -> String {
    let mut output = words(value)
        .into_iter()
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| format!("{}{}", first.to_ascii_uppercase(), chars.as_str()))
                .unwrap_or_default()
        })
        .collect::<String>();
    if output.is_empty() {
        output = "GeneratedModel".into();
    }
    if output.starts_with(|character: char| character.is_ascii_digit()) {
        output.insert(0, 'M');
    }
    output
}

fn camel_identifier(value: &str) -> String {
    let pascal = pascal_identifier(value);
    let output = lower_first(&pascal);
    if matches!(
        output.as_str(),
        "default" | "delete" | "function" | "import" | "model" | "null" | "return" | "type"
    ) {
        format!("{output}Field")
    } else {
        output
    }
}

fn lower_first(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| format!("{}{}", first.to_ascii_lowercase(), chars.as_str()))
        .unwrap_or_default()
}

fn default_control(kind: CanonicalType) -> &'static str {
    match kind {
        CanonicalType::Integer | CanonicalType::Real | CanonicalType::Decimal => "number",
        CanonicalType::Boolean => "checkbox",
        CanonicalType::Date | CanonicalType::DateTime => "date",
        CanonicalType::Json => "textarea",
        _ => "text",
    }
}

fn prisma_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}
fn dotenv_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn escape_tsx(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('{', "&#123;")
}

fn render_entity(entity: &EntitySpec, auth_enabled: bool) -> Vec<GeneratedFile> {
    let feature = format!("src/features/{}", entity.slug);
    let route = format!("src/app/(dashboard)/{}", entity.slug);
    vec![
        generated(&format!("{feature}/schema.ts"), entity_schema(entity)),
        generated(
            &format!("{feature}/actions.ts"),
            entity_actions(entity, auth_enabled),
        ),
        generated(
            &format!("{feature}/query.ts"),
            entity_query(entity, auth_enabled),
        ),
        generated(&format!("{feature}/form.tsx"), entity_form(entity)),
        generated(&format!("{feature}/table.tsx"), entity_table(entity)),
        generated(&format!("{route}/page.tsx"), entity_list_page(entity)),
        generated(
            &format!("{route}/create/page.tsx"),
            entity_create_page(entity),
        ),
        generated(&format!("{route}/[id]/page.tsx"), entity_view_page(entity)),
        generated(
            &format!("{route}/[id]/edit/page.tsx"),
            entity_edit_page(entity),
        ),
        generated(&format!("{feature}/metadata.ts"), entity_metadata(entity)),
    ]
}

fn entity_schema(entity: &EntitySpec) -> String {
    let fields = entity
        .fields
        .iter()
        .filter(|field| field.show_form)
        .map(|field| format!("  {}: {},", field.prisma, zod_expression(field)))
        .collect::<Vec<_>>()
        .join("\n");
    format!("import {{ z }} from \"zod\";\n\nexport const {delegate}Schema = z.object({{\n{fields}\n}});\nexport type {model}FormInput = z.input<typeof {delegate}Schema>;\nexport type {model}Input = z.output<typeof {delegate}Schema>;\n", delegate = entity.delegate, model = entity.model)
}

fn zod_expression(field: &FieldSpec) -> String {
    let mut value: String = match field.kind {
        CanonicalType::Integer => "z.coerce.number().int()".into(),
        CanonicalType::Real | CanonicalType::Decimal => "z.coerce.number()".into(),
        CanonicalType::Boolean => "z.coerce.boolean()".into(),
        CanonicalType::Date | CanonicalType::DateTime => "z.coerce.date()".into(),
        CanonicalType::Bytes => "z.instanceof(Uint8Array)".into(),
        CanonicalType::Json => "z.union([z.string(), z.record(z.string(), z.unknown())])".into(),
        CanonicalType::Text | CanonicalType::Unknown => "z.string()".into(),
    };
    for (kind, raw, message) in &field.validations {
        let custom = message
            .as_ref()
            .map(|value| format!(", {{ message: {} }}", js_string(value)))
            .unwrap_or_default();
        match kind.as_str() {
            "minLength" => {
                if let Some(number) = json_number(raw.as_deref()) {
                    value.push_str(&format!(".min({number}{custom})"));
                }
            }
            "maxLength" => {
                if let Some(number) = json_number(raw.as_deref()) {
                    value.push_str(&format!(".max({number}{custom})"));
                }
            }
            "min" => {
                if let Some(number) = json_number(raw.as_deref()) {
                    value.push_str(&format!(".min({number}{custom})"));
                }
            }
            "max" => {
                if let Some(number) = json_number(raw.as_deref()) {
                    value.push_str(&format!(".max({number}{custom})"));
                }
            }
            "email" => value.push_str(&format!(
                ".email({})",
                message
                    .as_ref()
                    .map_or("undefined".into(), |value| js_string(value))
            )),
            "pattern" => {
                if let Some(pattern) = raw
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<String>(value).ok())
                {
                    value.push_str(&format!(
                        ".regex(new RegExp({}){})",
                        js_string(&pattern),
                        custom
                    ));
                }
            }
            _ => unreachable!("validation rules are checked before rendering"),
        }
    }
    if !field.required || field.nullable {
        value.push_str(".optional().nullable()");
    }
    value
}

fn json_number(value: Option<&str>) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(value?).ok()?;
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .map(|number| number.to_string())
}

fn entity_actions(entity: &EntitySpec, auth_enabled: bool) -> String {
    let id_parser = id_parser(&entity.primary, "id");
    let guard_import = if auth_enabled {
        "import { requirePermission } from \"@/lib/access\";\n"
    } else {
        ""
    };
    let guard = |action: &str| {
        if auth_enabled {
            format!(
                "  await requirePermission({}, {});\n",
                js_string(&entity.key),
                js_string(action)
            )
        } else {
            String::new()
        }
    };
    format!(
        r#""use server";
import {{ revalidatePath }} from "next/cache";
import {{ prisma }} from "@/lib/prisma";
import {{ {delegate}Schema }} from "./schema";
{guard_import}

export type ActionResult = {{ ok: true }} | {{ ok: false; errors: Record<string,string[]> }};
const invalid = (error: {{ flatten: () => {{ fieldErrors: Record<string,string[] | undefined> }} }}): ActionResult => {{
  const entries = Object.entries(error.flatten().fieldErrors).map(([key,value]) => [key,value ?? []]);
  return {{ ok: false, errors: Object.fromEntries(entries) }};
}};

export async function create{model}(input: unknown): Promise<ActionResult> {{
{create_guard}
  const parsed = {delegate}Schema.safeParse(input); if (!parsed.success) return invalid(parsed.error);
  await prisma.{delegate}.create({{ data: parsed.data as never }}); revalidatePath("/{slug}"); return {{ ok: true }};
}}
export async function update{model}(id: string, input: unknown): Promise<ActionResult> {{
{update_guard}
  const parsed = {delegate}Schema.safeParse(input); if (!parsed.success) return invalid(parsed.error);
  await prisma.{delegate}.update({{ where: {{ {pk}: {id_parser} }}, data: parsed.data as never }}); revalidatePath("/{slug}"); return {{ ok: true }};
}}
export async function delete{model}(id: string): Promise<void> {{
{delete_guard}
  await prisma.{delegate}.delete({{ where: {{ {pk}: {id_parser} }} }}); revalidatePath("/{slug}");
}}
"#,
        delegate = entity.delegate,
        model = entity.model,
        slug = entity.slug,
        pk = entity.primary.prisma,
        guard_import = guard_import,
        create_guard = guard("create"),
        update_guard = guard("update"),
        delete_guard = guard("delete")
    )
}

fn entity_query(entity: &EntitySpec, auth_enabled: bool) -> String {
    let policy_fields = entity
        .fields
        .iter()
        .map(|field| {
            format!(
                "    {}: {},",
                js_string(&field.prisma),
                js_string(query_kind(field.kind))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let searchable = entity
        .fields
        .iter()
        .filter(|field| matches!(field.kind, CanonicalType::Text | CanonicalType::Unknown))
        .map(|field| js_string(&field.prisma))
        .collect::<Vec<_>>()
        .join(", ");
    let sortable = entity
        .fields
        .iter()
        .map(|field| js_string(&field.prisma))
        .collect::<Vec<_>>()
        .join(", ");
    let id_parser = id_parser(&entity.primary, "id");
    let guard_import = if auth_enabled {
        "import { requirePermission } from \"@/lib/access\";\n"
    } else {
        ""
    };
    let read_guard = if auth_enabled {
        format!(
            "  await requirePermission({}, \"read\");\n",
            js_string(&entity.key)
        )
    } else {
        String::new()
    };
    format!(
        r#"import {{ prisma }} from "@/lib/prisma";
import {{ parseListQuery }} from "@/lib/query-contract";
{guard_import}

export const {delegate}QueryPolicy = {{
  fields: {{
{policy_fields}
  }}, searchable: [{searchable}], sortable: [{sortable}], maxPageSize: 100
}} as const;

const scalar = (kind: string, value: string): unknown => {{ if (kind === "number") return Number(value); if (kind === "boolean") return value === "true"; if (kind === "date") return new Date(value); return value; }};
const serialise = (row: object): Record<string,string | number | boolean | null> => Object.fromEntries(Object.entries(row).map(([key,value]) => [key, value instanceof Date ? value.toISOString() : typeof value === "bigint" ? value.toString() : value])) as Record<string,string | number | boolean | null>;

export async function list{model}(params: URLSearchParams) {{
{read_guard}
  const query = parseListQuery(params, {delegate}QueryPolicy); const and: Record<string,unknown>[] = [];
  if (query.search && {delegate}QueryPolicy.searchable.length) and.push({{ OR: {delegate}QueryPolicy.searchable.map((field) => ({{ [field]: {{ contains: query.search }} }})) }});
  for (const filter of query.filters) {{ const kind = {delegate}QueryPolicy.fields[filter.field as keyof typeof {delegate}QueryPolicy.fields]; and.push({{ [filter.field]: {{ [filter.operator]: scalar(kind, filter.value) }} }}); }}
  const where = and.length ? {{ AND: and }} : {{}}; const orderBy = query.sort ? {{ [query.sort]: query.direction }} : undefined;
  const [rows,total] = await Promise.all([prisma.{delegate}.findMany({{ where: where as never, orderBy: orderBy as never, skip: (query.page - 1) * query.pageSize, take: query.pageSize }}), prisma.{delegate}.count({{ where: where as never }})]);
  return {{ rows: rows.map(serialise), total, query }};
}}
export async function get{model}(id: string) {{
{read_guard}  const row = await prisma.{delegate}.findUnique({{ where: {{ {pk}: {id_parser} }} }}); return row ? serialise(row) : null; }}
"#,
        delegate = entity.delegate,
        model = entity.model,
        policy_fields = policy_fields,
        searchable = searchable,
        sortable = sortable,
        pk = entity.primary.prisma,
        guard_import = guard_import,
        read_guard = read_guard
    )
}

fn entity_form(entity: &EntitySpec) -> String {
    let imports = format!("import {{ {delegate}Schema, type {model}FormInput, type {model}Input }} from \"./schema\";\nimport {{ create{model}, update{model} }} from \"./actions\";\nimport {{ Button }} from \"@/components/ui/button\";\nimport {{ Card, CardContent, CardFooter }} from \"@/components/ui/card\";\nimport {{ Input }} from \"@/components/ui/input\";\nimport {{ Label }} from \"@/components/ui/label\";\nimport {{ Textarea }} from \"@/components/ui/textarea\";", delegate=entity.delegate, model=entity.model);
    let controls = entity
        .fields
        .iter()
        .filter(|field| field.show_form)
        .map(form_control)
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#""use client";
import {{ useState }} from "react";
import {{ useRouter }} from "next/navigation";
import {{ useForm }} from "react-hook-form";
import {{ zodResolver }} from "@hookform/resolvers/zod";
{imports}

export function {model}Form({{ id, initial }}: {{ id?: string; initial?: Record<string,unknown> }}) {{
  const router = useRouter(); const [serverError,setServerError] = useState("");
  const {{ register, handleSubmit, formState: {{ errors, isSubmitting }} }} = useForm<{model}FormInput, unknown, {model}Input>({{ resolver: zodResolver({delegate}Schema), defaultValues: initial as never }});
  const submit = handleSubmit(async (values) => {{ setServerError(""); const result = id ? await update{model}(id, values) : await create{model}(values); if (!result.ok) {{ setServerError(Object.values(result.errors).flat().join("; ")); return; }} router.push("/{slug}"); router.refresh(); }});
  return <Card className="mt-6 max-w-2xl border-border/70 shadow-sm"><form onSubmit={{submit}}><CardContent className="grid gap-5 p-5 sm:p-6">{controls}{{serverError && <p className="text-sm text-destructive" role="alert">{{serverError}}</p>}}</CardContent><CardFooter className="justify-end border-t bg-muted/20 px-5 py-4 sm:px-6"><Button disabled={{isSubmitting}} type="submit">{{isSubmitting ? "Saving..." : "Save changes"}}</Button></CardFooter></form></Card>;
}}
"#,
        imports = imports,
        model = entity.model,
        delegate = entity.delegate,
        slug = entity.slug,
        controls = controls
    )
}

fn form_control(field: &FieldSpec) -> String {
    let key = &field.prisma;
    let label = escape_tsx(&field.key);
    let error = format!("{{errors.{key}?.message && <span className=\"text-sm text-destructive\">{{String(errors.{key}?.message)}}</span>}}");
    let input = match field.control.as_str() {
        "textarea" => format!("<Textarea id=\"{key}\" {{...register(\"{key}\")}} />"),
        "checkbox" => format!("<Input id=\"{key}\" className=\"h-4 w-4\" type=\"checkbox\" {{...register(\"{key}\")}} />"),
        "date" => format!("<Input id=\"{key}\" type=\"datetime-local\" {{...register(\"{key}\")}} />"),
        "number" => format!("<Input id=\"{key}\" type=\"number\" step=\"any\" {{...register(\"{key}\")}} />"),
        _ => format!("<Input id=\"{key}\" type=\"text\" {{...register(\"{key}\")}} />"),
    };
    format!("<div className=\"grid gap-2\"><Label htmlFor=\"{key}\">{label}</Label>{input}{error}</div>")
}

fn entity_table(entity: &EntitySpec) -> String {
    let column_count = entity.fields.iter().filter(|field| field.show_list).count() + 1;
    let columns = entity
        .fields
        .iter()
        .filter(|field| field.show_list)
        .map(|field| {
            format!(
                "  {{ accessorKey: {}, header: {} }},",
                js_string(&field.prisma),
                js_string(&field.key)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"/* eslint-disable react-hooks/incompatible-library -- TanStack Table intentionally exposes non-memoizable functions. */
"use client";
import Link from "next/link";
import {{ createColumnHelper, flexRender, getCoreRowModel, useReactTable }} from "@tanstack/react-table";
import {{ Table, TableBody, TableCell, TableHead, TableHeader, TableRow }} from "@/components/ui/table";
type Row = Record<string,string | number | boolean | null>;
const helper = createColumnHelper<Row>();
const columns = [
{columns}
].map((column) => helper.accessor(column.accessorKey, {{ header: column.header, cell: (context) => String(context.getValue() ?? "") }}));
export function {model}Table({{ rows }}: {{ rows: Row[] }}) {{ const table = useReactTable({{ data: rows, columns, getCoreRowModel: getCoreRowModel() }}); return <Table><TableHeader>{{table.getHeaderGroups().map((group) => <TableRow key={{group.id}}>{{group.headers.map((header) => <TableHead key={{header.id}}>{{flexRender(header.column.columnDef.header, header.getContext())}}</TableHead>)}}<TableHead className="w-28 text-right">Actions</TableHead></TableRow>)}}</TableHeader><TableBody>{{table.getRowModel().rows.length === 0 ? <TableRow><TableCell className="h-28 text-center text-muted-foreground" colSpan={{{column_count}}}>No records found.</TableCell></TableRow> : table.getRowModel().rows.map((row) => <TableRow key={{row.id}}>{{row.getVisibleCells().map((cell) => <TableCell key={{cell.id}}>{{flexRender(cell.column.columnDef.cell, cell.getContext())}}</TableCell>)}}<TableCell><div className="flex justify-end gap-3"><Link className="text-sm font-medium text-foreground/80 transition-colors hover:text-foreground" href={{`/{slug}/${{String(row.original.{pk})}}`}}>View</Link><Link className="text-sm font-medium text-foreground/80 transition-colors hover:text-foreground" href={{`/{slug}/${{String(row.original.{pk})}}/edit`}}>Edit</Link></div></TableCell></TableRow>)}}</TableBody></Table>; }}
"#,
        columns = columns,
        model = entity.model,
        slug = entity.slug,
        pk = entity.primary.prisma,
        column_count = column_count
    )
}

fn entity_list_page(entity: &EntitySpec) -> String {
    format!(
        r#"import Link from "next/link";
import {{ Plus, Search }} from "lucide-react";
import {{ list{model} }} from "@/features/{slug}/query";
import {{ {model}Table }} from "@/features/{slug}/table";
import {{ Button }} from "@/components/ui/button";
import {{ Card, CardContent }} from "@/components/ui/card";
import {{ Input }} from "@/components/ui/input";
export const dynamic = "force-dynamic";
export default async function Page({{ searchParams }}: {{ searchParams: Promise<Record<string,string | string[] | undefined>> }}) {{ const raw = await searchParams; const params = new URLSearchParams(); for (const [key,value] of Object.entries(raw)) if (typeof value === "string") params.set(key,value); const result = await list{model}(params); return <section className="mx-auto max-w-6xl space-y-5"><div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end"><div><p className="text-xs font-medium uppercase tracking-widest text-muted-foreground">Records</p><h1 className="mt-1 text-2xl font-semibold tracking-tight">{label}</h1><p className="mt-1 text-sm text-muted-foreground">Manage your {label_lower} records.</p></div><Link className="inline-flex h-9 items-center justify-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground shadow-sm transition-colors hover:bg-primary/90" href="/{slug}/create"><Plus className="size-4" />Create</Link></div><Card className="border-border/70 shadow-sm"><CardContent className="p-3 sm:p-4"><form className="flex gap-2"><div className="relative flex-1"><Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" /><Input className="pl-9" aria-label="Search" name="search" defaultValue={{result.query.search}} placeholder="Search records..."/></div><Button type="submit" variant="outline">Search</Button></form></CardContent><div className="border-t"><{model}Table rows={{result.rows}}/></div><div className="flex items-center justify-between border-t px-4 py-3 text-sm text-muted-foreground"><span>{{result.total}} records</span><span>Page {{result.query.page}}</span></div></Card></section>; }}
"#,
        model = entity.model,
        slug = entity.slug,
        label = escape_tsx(&entity.label),
        label_lower = escape_tsx(&entity.label.to_lowercase())
    )
}

fn entity_create_page(entity: &EntitySpec) -> String {
    format!("import {{ {model}Form }} from \"@/features/{slug}/form\";\nexport default function Page() {{ return <section className=\"mx-auto max-w-6xl\"><p className=\"text-xs font-medium uppercase tracking-widest text-muted-foreground\">Create record</p><h1 className=\"mt-1 text-2xl font-semibold tracking-tight\">New {label}</h1><p className=\"mt-1 text-sm text-muted-foreground\">Add a new record to this collection.</p><{model}Form /></section>; }}\n", model=entity.model, slug=entity.slug, label=escape_tsx(&entity.label))
}

fn entity_view_page(entity: &EntitySpec) -> String {
    let rows = entity
        .fields
        .iter()
        .filter(|field| field.show_view)
        .map(|field| {
            format!(
                "<><dt>{}</dt><dd>{{String(row.{} ?? \"\")}}</dd></>",
                escape_tsx(&field.key),
                field.prisma
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!(
        r#"import {{ notFound, redirect }} from "next/navigation";
import {{ get{model} }} from "@/features/{slug}/query";
import {{ delete{model} }} from "@/features/{slug}/actions";
import {{ Card, CardContent }} from "@/components/ui/card";
export const dynamic = "force-dynamic";
export default async function Page({{ params }}: {{ params: Promise<{{ id: string }}> }}) {{ const {{ id }} = await params; const row = await get{model}(id); if (!row) notFound(); async function remove() {{ "use server"; await delete{model}(id); redirect("/{slug}"); }} return <section className="mx-auto max-w-6xl space-y-5"><div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end"><div><p className="text-xs font-medium uppercase tracking-widest text-muted-foreground">Record detail</p><h1 className="mt-1 text-2xl font-semibold tracking-tight">{label}</h1></div><div className="flex gap-2"><a className="inline-flex h-9 items-center justify-center rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground shadow-sm hover:bg-primary/90" href={{`/{slug}/${{id}}/edit`}}>Edit</a><form action={{remove}}><button className="inline-flex h-9 items-center justify-center rounded-md bg-destructive px-3 text-sm font-medium text-destructive-foreground shadow-sm hover:bg-destructive/90" type="submit">Delete</button></form></div></div><Card className="max-w-3xl border-border/70 shadow-sm"><CardContent className="p-0"><dl className="grid grid-cols-[minmax(9rem,1fr)_2fr] text-sm [&_dd]:border-b [&_dd]:p-3.5 [&_dt]:border-b [&_dt]:bg-muted/30 [&_dt]:p-3.5 [&_dt]:font-medium">{rows}</dl></CardContent></Card></section>; }}
"#,
        model = entity.model,
        slug = entity.slug,
        label = escape_tsx(&entity.label),
        rows = rows
    )
}

fn entity_edit_page(entity: &EntitySpec) -> String {
    format!(
        r#"import {{ notFound }} from "next/navigation";
import {{ get{model} }} from "@/features/{slug}/query";
import {{ {model}Form }} from "@/features/{slug}/form";
export const dynamic = "force-dynamic";
export default async function Page({{ params }}: {{ params: Promise<{{ id: string }}> }}) {{ const {{ id }} = await params; const row = await get{model}(id); if (!row) notFound(); return <section className="mx-auto max-w-6xl"><p className="text-xs font-medium uppercase tracking-widest text-muted-foreground">Edit record</p><h1 className="mt-1 text-2xl font-semibold tracking-tight">Edit {label}</h1><p className="mt-1 text-sm text-muted-foreground">Update the selected record.</p><{model}Form id={{id}} initial={{row}} /></section>; }}
"#,
        model = entity.model,
        slug = entity.slug,
        label = escape_tsx(&entity.label)
    )
}

fn entity_metadata(entity: &EntitySpec) -> String {
    let fields = entity
        .fields
        .iter()
        .map(|field| {
            format!(
                "    {{ key: {}, column: {}, type: {}, list: {}, view: {}, form: {} }},",
                js_string(&field.key),
                js_string(&field.column),
                js_string(query_kind(field.kind)),
                field.show_list,
                field.show_view,
                field.show_form
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("export const entityMetadata = {{\n  key: {}, table: {}, route: {}, primaryKey: {}, fields: [\n{fields}\n  ]\n}} as const;\n", js_string(&entity.key), js_string(&entity.table), js_string(&entity.slug), js_string(&entity.primary.prisma))
}

fn entity_crud_test(entity: &EntitySpec) -> String {
    let columns = entity
        .fields
        .iter()
        .map(|field| {
            let kind = match field.kind {
                CanonicalType::Integer | CanonicalType::Boolean => "INTEGER",
                CanonicalType::Real | CanonicalType::Decimal => "REAL",
                CanonicalType::Bytes => "BLOB",
                _ => "TEXT",
            };
            let primary = if field.primary {
                if field.kind == CanonicalType::Integer {
                    " PRIMARY KEY AUTOINCREMENT"
                } else {
                    " PRIMARY KEY"
                }
            } else {
                ""
            };
            let required = if !field.nullable && !field.primary {
                " NOT NULL"
            } else {
                ""
            };
            format!(
                "{} {kind}{primary}{required}",
                sqlite_identifier(&field.column)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let create_values = test_input(entity, false);
    let update_values = test_input(entity, true);
    let invalid_field = entity
        .fields
        .iter()
        .find(|field| {
            field.show_form
                && field.required
                && !field.nullable
                && matches!(field.kind, CanonicalType::Text | CanonicalType::Unknown)
        })
        .expect("CRUD test eligibility requires a text field");
    let assertion_field = entity
        .fields
        .iter()
        .find(|field| field.show_form && !field.primary)
        .unwrap_or(invalid_field);
    let expected = test_value(assertion_field, true);
    format!(
        r#"import Database from "better-sqlite3";
import {{ rmSync }} from "node:fs";
import {{ tmpdir }} from "node:os";
import {{ join }} from "node:path";
import {{ afterAll, describe, expect, it, vi }} from "vitest";

vi.mock("next/cache", () => ({{ revalidatePath: vi.fn() }}));
const databasePath = join(tmpdir(), `emanduite-crud-${{process.pid}}.sqlite`);
rmSync(databasePath, {{ force: true }});
new Database(databasePath).exec({create_sql});
process.env.DATABASE_URL = `file:${{databasePath.replaceAll("\\", "/")}}`;
const nonce = Math.floor(Date.now() % 1_000_000_000);

describe("generated {model} CRUD", () => {{
  it("applies server validation and create/update/delete against isolated SQLite", async () => {{
    const actions = await import("./actions");
    const {{ prisma }} = await import("@/lib/prisma");
    const before = await prisma.{delegate}.count();
    const invalid = await actions.create{model}({{ {invalid}: null }});
    expect(invalid.ok).toBe(false);

    const createdResult = await actions.create{model}({create_values});
    expect(createdResult).toEqual({{ ok: true }});
    expect(await prisma.{delegate}.count()).toBe(before + 1);
    const created = await prisma.{delegate}.findFirst({{ orderBy: {{ {pk}: "desc" }} }});
    expect(created).not.toBeNull();

    const id = String(created!.{pk});
    const updatedResult = await actions.update{model}(id, {update_values});
    expect(updatedResult).toEqual({{ ok: true }});
    const updated = await prisma.{delegate}.findUnique({{ where: {{ {pk}: created!.{pk} }} }});
    expect(updated?.{assertion}).toEqual({expected});

    await actions.delete{model}(id);
    expect(await prisma.{delegate}.count()).toBe(before);
  }});
}});

afterAll(async () => {{
  const {{ prisma }} = await import("@/lib/prisma");
  await prisma.$disconnect();
  rmSync(databasePath, {{ force: true }});
}});
"#,
        create_sql = js_string(&format!(
            "CREATE TABLE {} ({columns})",
            sqlite_identifier(&entity.table)
        )),
        model = entity.model,
        delegate = entity.delegate,
        invalid = invalid_field.prisma,
        create_values = create_values,
        update_values = update_values,
        pk = entity.primary.prisma,
        assertion = assertion_field.prisma,
        expected = expected,
    )
}

fn test_input(entity: &EntitySpec, updated: bool) -> String {
    let values = entity
        .fields
        .iter()
        .filter(|field| field.show_form)
        .map(|field| format!("{}: {}", field.prisma, test_value(field, updated)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{ {values} }}")
}

fn test_value(field: &FieldSpec, updated: bool) -> String {
    if field.primary && updated {
        return test_value(field, false);
    }
    match field.kind {
        CanonicalType::Integer => if updated { "nonce + 1" } else { "nonce" }.into(),
        CanonicalType::Real | CanonicalType::Decimal => {
            if updated { "84.5" } else { "41.5" }.into()
        }
        CanonicalType::Boolean => (!updated).to_string(),
        CanonicalType::Date | CanonicalType::DateTime => if updated {
            "new Date(\"2026-07-20T00:00:00.000Z\")"
        } else {
            "new Date(\"2026-07-19T00:00:00.000Z\")"
        }
        .into(),
        CanonicalType::Bytes => if updated {
            "new Uint8Array([5, 2])"
        } else {
            "new Uint8Array([5, 1])"
        }
        .into(),
        CanonicalType::Json => if updated {
            "{ phase: 5, updated: true }"
        } else {
            "{ phase: 5 }"
        }
        .into(),
        CanonicalType::Text | CanonicalType::Unknown => format!(
            "`phase5-${{nonce}}-{}{} ` .trim()",
            field.prisma,
            if updated { "-updated" } else { "" }
        ),
    }
}

fn sqlite_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn query_kind(kind: CanonicalType) -> &'static str {
    match kind {
        CanonicalType::Integer | CanonicalType::Real | CanonicalType::Decimal => "number",
        CanonicalType::Boolean => "boolean",
        CanonicalType::Date | CanonicalType::DateTime => "date",
        _ => "string",
    }
}

fn id_parser(field: &FieldSpec, variable: &str) -> String {
    match field.kind {
        CanonicalType::Integer | CanonicalType::Real | CanonicalType::Decimal => {
            format!("Number({variable})")
        }
        _ => variable.into(),
    }
}

const TSCONFIG: &str = r#"{
  "compilerOptions": {
    "target": "ES2022", "lib": ["dom", "dom.iterable", "esnext"], "strict": true, "allowJs": true,
    "noEmit": true, "skipLibCheck": true, "esModuleInterop": true, "module": "esnext", "moduleResolution": "bundler",
    "resolveJsonModule": true, "isolatedModules": true, "jsx": "react-jsx",
    "incremental": true, "plugins": [{ "name": "next" }], "paths": { "@/*": ["./src/*"] },
    "types": ["node"]
  },
  "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts", ".next/dev/types/**/*.ts"],
  "exclude": ["node_modules"]
}"#;

const PRISMA_CONFIG: &str = r#"import "dotenv/config";
import { defineConfig, env } from "prisma/config";

export default defineConfig({
  schema: "prisma/schema.prisma",
  migrations: { path: "prisma/migrations" },
  datasource: { url: env("DATABASE_URL") }
});"#;

fn prisma_client(provider: DatabaseProvider) -> String {
    match provider {
        DatabaseProvider::Sqlite => r#"import { PrismaBetterSqlite3 } from "@prisma/adapter-better-sqlite3";
import { PrismaClient } from "@/generated/prisma/client";

const globalForPrisma = globalThis as unknown as { prisma?: PrismaClient };
const adapter = new PrismaBetterSqlite3({ url: process.env.DATABASE_URL ?? "file:./prisma/dev.sqlite" });
export const prisma = globalForPrisma.prisma ?? new PrismaClient({ adapter });
if (process.env.NODE_ENV !== "production") globalForPrisma.prisma = prisma;"#.into(),
        DatabaseProvider::Postgresql | DatabaseProvider::Mysql => r#"import { PrismaClient } from "@/generated/prisma/client";

const globalForPrisma = globalThis as unknown as { prisma?: PrismaClient };
export const prisma = globalForPrisma.prisma ?? new PrismaClient();
if (process.env.NODE_ENV !== "production") globalForPrisma.prisma = prisma;"#.into(),
    }
}

const SYSTEM_MODELS: &str = r#"
model SysRole {
  id String @id @default(cuid())
  key String @unique
  label String
  @@map("sys_roles")
}

model SysAuthSubject {
  id String @id @default(cuid())
  externalId String @unique @map("external_id")
  roleKey String @map("role_key")
  createdAt DateTime @default(now()) @map("created_at")
  @@map("sys_auth_subjects")
}

model SysResource {
  id String @id @default(cuid())
  key String @unique
  @@map("sys_resources")
}

model SysPermission {
  id String @id @default(cuid())
  roleKey String @map("role_key")
  resourceKey String @map("resource_key")
  action String
  @@unique([roleKey, resourceKey, action])
  @@map("sys_permissions")
}

model SysAuditLog {
  id String @id @default(cuid())
  subjectId String? @map("subject_id")
  resourceKey String @map("resource_key")
  action String
  outcome String
  createdAt DateTime @default(now()) @map("created_at")
  @@index([resourceKey, createdAt])
  @@map("sys_audit_logs")
}
"#;

const HOOK_RUNTIME: &str = r#"export interface HookContextV1<TInput = unknown> { version: 1; entity: string; action: "create" | "update" | "delete" | "list" | "view"; input: TInput; }
export interface HookOutcome { name: string; ok: boolean; timedOut: boolean; }
export async function runHook<T>(name: string, hook: ((context: HookContextV1<T>) => Promise<T>) | undefined, context: HookContextV1<T>, timeoutMs = 2_000): Promise<{ value: T; outcome: HookOutcome }> {
  if (!hook) return { value: context.input, outcome: { name, ok: true, timedOut: false } };
  let timer: ReturnType<typeof setTimeout> | undefined;
  try { const value = await Promise.race([hook(context), new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("hook timeout")), timeoutMs); })]); return { value, outcome: { name, ok: true, timedOut: false } }; }
  catch { return { value: context.input, outcome: { name, ok: false, timedOut: true } }; }
  finally { if (timer) clearTimeout(timer); }
}"#;

const EXTENSION_TYPES: &str = r#"export interface HookContextV1<TInput = unknown> {
  version: 1;
  entity: string;
  action: "create" | "update" | "delete" | "list" | "view";
  input: TInput;
}

export type BeforeInputHook<TInput> = (context: HookContextV1<TInput>) => Promise<TInput>;
export type ExtensionRegistry = Readonly<Record<string, string>>;"#;

const LAYOUT: &str = r#"import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = { title: "{{PROJECT_NAME}}", description: "Generated by Emanduite" };

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return <html lang="en"><body>{children}</body></html>;
}"#;

const SHADCN_COMPONENTS_JSON: &str = r#"{
  "$schema": "https://ui.shadcn.com/schema.json",
  "style": "new-york",
  "rsc": true,
  "tsx": true,
  "tailwind": { "config": "", "css": "src/app/globals.css", "baseColor": "neutral", "cssVariables": true, "prefix": "" },
  "aliases": { "components": "@/components", "utils": "@/lib/utils", "ui": "@/components/ui", "lib": "@/lib", "hooks": "@/hooks" },
  "iconLibrary": "lucide"
}"#;

const SHADCN_UTILS: &str = r#"import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
export function cn(...inputs: ClassValue[]) { return twMerge(clsx(inputs)); }
"#;

const SHADCN_BUTTON: &str = r#"import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva("inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-colors disabled:pointer-events-none disabled:opacity-50", { variants: { variant: { default: "bg-primary text-primary-foreground shadow hover:bg-primary/90", destructive: "bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90", outline: "border border-input bg-background shadow-sm hover:bg-accent hover:text-accent-foreground", secondary: "bg-secondary text-secondary-foreground shadow-sm hover:bg-secondary/80", ghost: "hover:bg-accent hover:text-accent-foreground", link: "text-primary underline-offset-4 hover:underline" }, size: { default: "h-9 px-4 py-2", sm: "h-8 rounded-md px-3 text-xs", lg: "h-10 rounded-md px-8", icon: "h-9 w-9" } }, defaultVariants: { variant: "default", size: "default" } });
export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonVariants> {}
export function Button({ className, variant, size, ...props }: ButtonProps) { return <button data-slot="button" className={cn(buttonVariants({ variant, size }), className)} {...props} />; }
"#;

const SHADCN_CARD: &str = r#"import { cn } from "@/lib/utils";
export function Card({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) { return <div data-slot="card" className={cn("rounded-xl border bg-card text-card-foreground shadow", className)} {...props} />; }
export function CardHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) { return <div data-slot="card-header" className={cn("flex flex-col space-y-1.5 p-6", className)} {...props} />; }
export function CardTitle({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) { return <h3 data-slot="card-title" className={cn("font-semibold leading-none tracking-tight", className)} {...props} />; }
export function CardDescription({ className, ...props }: React.HTMLAttributes<HTMLParagraphElement>) { return <p data-slot="card-description" className={cn("text-sm text-muted-foreground", className)} {...props} />; }
export function CardContent({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) { return <div data-slot="card-content" className={cn("p-6 pt-0", className)} {...props} />; }
export function CardFooter({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) { return <div data-slot="card-footer" className={cn("flex items-center p-6 pt-0", className)} {...props} />; }
"#;

const SHADCN_INPUT: &str = r#"import { cn } from "@/lib/utils";
export function Input({ className, type, ...props }: React.InputHTMLAttributes<HTMLInputElement>) { return <input type={type} data-slot="input" className={cn("flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50", className)} {...props} />; }
"#;

const SHADCN_TEXTAREA: &str = r#"import { cn } from "@/lib/utils";
export function Textarea({ className, ...props }: React.TextareaHTMLAttributes<HTMLTextAreaElement>) { return <textarea data-slot="textarea" className={cn("flex min-h-20 w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50", className)} {...props} />; }
"#;

const SHADCN_LABEL: &str = r#"import { cn } from "@/lib/utils";
export function Label({ className, ...props }: React.LabelHTMLAttributes<HTMLLabelElement>) { return <label data-slot="label" className={cn("text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70", className)} {...props} />; }
"#;

const SHADCN_TABLE: &str = r#"import { cn } from "@/lib/utils";
export function Table({ className, ...props }: React.TableHTMLAttributes<HTMLTableElement>) { return <div data-slot="table-container" className="relative w-full overflow-auto"><table data-slot="table" className={cn("w-full caption-bottom text-sm", className)} {...props} /></div>; }
export function TableHeader({ className, ...props }: React.HTMLAttributes<HTMLTableSectionElement>) { return <thead data-slot="table-header" className={cn("[&_tr]:border-b", className)} {...props} />; }
export function TableBody({ className, ...props }: React.HTMLAttributes<HTMLTableSectionElement>) { return <tbody data-slot="table-body" className={cn("[&_tr:last-child]:border-0", className)} {...props} />; }
export function TableRow({ className, ...props }: React.HTMLAttributes<HTMLTableRowElement>) { return <tr data-slot="table-row" className={cn("border-b transition-colors hover:bg-muted/50", className)} {...props} />; }
export function TableHead({ className, ...props }: React.ThHTMLAttributes<HTMLTableCellElement>) { return <th data-slot="table-head" className={cn("h-10 px-2 text-left align-middle font-medium text-muted-foreground", className)} {...props} />; }
export function TableCell({ className, ...props }: React.TdHTMLAttributes<HTMLTableCellElement>) { return <td data-slot="table-cell" className={cn("p-2 align-middle", className)} {...props} />; }
"#;

const DASHBOARD_LAYOUT: &str = r#"import Link from "next/link";
import { Boxes } from "lucide-react";
import { AppSidebar } from "@/components/app-sidebar";
export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  return <div className="min-h-screen bg-muted/30"><AppSidebar /><div className="md:pl-60"><header className="sticky top-0 z-20 hidden h-16 items-center justify-between border-b bg-background/80 px-8 backdrop-blur md:flex"><div><p className="text-xs font-medium uppercase tracking-widest text-muted-foreground">Workspace</p><p className="text-sm font-semibold">{{PROJECT_NAME}}</p></div><div className="rounded-full border bg-background px-3 py-1.5 text-xs text-muted-foreground">Generated admin</div></header><header className="flex h-14 items-center border-b bg-background/80 px-5 backdrop-blur md:hidden"><Link className="flex items-center gap-2 text-sm font-semibold" href="/"><span className="grid size-7 place-items-center rounded-md bg-primary text-primary-foreground"><Boxes className="size-4" /></span>{{PROJECT_NAME}}</Link></header><main className="px-5 py-6 sm:px-8 sm:py-8">{children}</main></div></div>;
}"#;

const DASHBOARD_LAYOUT_AUTH: &str = r#"import Link from "next/link";
import { Boxes } from "lucide-react";
import { getServerSession } from "next-auth";
import { redirect } from "next/navigation";
import { authOptions } from "@/auth";
import { AppSidebar } from "@/components/app-sidebar";

export default async function DashboardLayout({ children }: { children: React.ReactNode }) {
  const session = await getServerSession(authOptions);
  if (!session?.user) redirect("/login");
  return <div className="min-h-screen bg-muted/30"><AppSidebar /><div className="md:pl-60"><header className="sticky top-0 z-20 hidden h-16 items-center justify-between border-b bg-background/80 px-8 backdrop-blur md:flex"><div><p className="text-xs font-medium uppercase tracking-widest text-muted-foreground">Workspace</p><p className="text-sm font-semibold">{{PROJECT_NAME}}</p></div><div className="rounded-full border bg-background px-3 py-1.5 text-xs text-muted-foreground">{session.user.name ?? "Administrator"}</div></header><header className="flex h-14 items-center border-b bg-background/80 px-5 backdrop-blur md:hidden"><Link className="flex items-center gap-2 text-sm font-semibold" href="/"><span className="grid size-7 place-items-center rounded-md bg-primary text-primary-foreground"><Boxes className="size-4" /></span>{{PROJECT_NAME}}</Link></header><main className="px-5 py-6 sm:px-8 sm:py-8">{children}</main></div></div>;
}"#;

const DASHBOARD_PAGE: &str = r#"import Link from "next/link";
import { ArrowUpRight, Database, FolderKanban } from "lucide-react";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
export default function DashboardPage() {
  return <section className="mx-auto max-w-6xl space-y-7"><div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end"><div><p className="text-xs font-medium uppercase tracking-widest text-muted-foreground">Workspace</p><h1 className="mt-1 text-2xl font-semibold tracking-tight">Overview</h1><p className="mt-1 text-sm text-muted-foreground">Your generated admin workspace is ready.</p></div><div className="flex items-center gap-2 text-sm text-muted-foreground"><FolderKanban className="size-4" />Blueprint v1</div></div><div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
{{ENTITY_CARDS}}
  </div></section>;
}"#;

const GLOBAL_CSS: &str = r#"@import "tailwindcss";

:root { --background: oklch(1 0 0); --foreground: oklch(.145 0 0); --card: oklch(1 0 0); --card-foreground: oklch(.145 0 0); --primary: oklch(.205 0 0); --primary-foreground: oklch(.985 0 0); --secondary: oklch(.97 0 0); --secondary-foreground: oklch(.205 0 0); --muted: oklch(.97 0 0); --muted-foreground: oklch(.556 0 0); --accent: oklch(.97 0 0); --accent-foreground: oklch(.205 0 0); --destructive: oklch(.577 .245 27.325); --destructive-foreground: oklch(.985 0 0); --border: oklch(.922 0 0); --input: oklch(.922 0 0); --ring: oklch(.708 0 0); --radius: .625rem; }
@theme inline { --color-background: var(--background); --color-foreground: var(--foreground); --color-card: var(--card); --color-card-foreground: var(--card-foreground); --color-primary: var(--primary); --color-primary-foreground: var(--primary-foreground); --color-secondary: var(--secondary); --color-secondary-foreground: var(--secondary-foreground); --color-muted: var(--muted); --color-muted-foreground: var(--muted-foreground); --color-accent: var(--accent); --color-accent-foreground: var(--accent-foreground); --color-destructive: var(--destructive); --color-destructive-foreground: var(--destructive-foreground); --color-border: var(--border); --color-input: var(--input); --color-ring: var(--ring); --radius-sm: calc(var(--radius) - 4px); --radius-md: calc(var(--radius) - 2px); --radius-lg: var(--radius); }
@layer base { * { @apply border-border; } body { @apply min-h-screen bg-background text-foreground antialiased; } }"#;

const QUERY_CONTRACT: &str = r#"export type ScalarKind = "string" | "number" | "boolean" | "date";
export type FilterOperator = "equals" | "contains" | "startsWith" | "lt" | "lte" | "gt" | "gte";
export interface QueryPolicy { fields: Readonly<Record<string, ScalarKind>>; searchable: readonly string[]; sortable: readonly string[]; maxPageSize: number; }
export interface ParsedFilter { field: string; operator: FilterOperator; value: string; }
export interface ParsedListQuery { page: number; pageSize: number; search: string; sort: string; direction: "asc" | "desc"; filters: ParsedFilter[]; }
const operators: Record<ScalarKind, readonly FilterOperator[]> = { string: ["equals","contains","startsWith"], number: ["equals","lt","lte","gt","gte"], boolean: ["equals"], date: ["equals","lt","lte","gt","gte"] };
const integer = (value: string | null, fallback: number) => { const parsed = Number(value); return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : fallback; };
export function parseListQuery(input: URLSearchParams, policy: QueryPolicy): ParsedListQuery {
  const page = integer(input.get("page"), 1); const pageSize = Math.min(integer(input.get("pageSize"), 20), policy.maxPageSize);
  const requestedSort = input.get("sort") ?? policy.sortable[0] ?? ""; const sort = policy.sortable.includes(requestedSort) ? requestedSort : (policy.sortable[0] ?? "");
  const direction = input.get("direction") === "desc" ? "desc" : "asc"; const search = (input.get("search") ?? "").trim().slice(0, 200);
  const filters: ParsedFilter[] = [];
  for (const [key,value] of input) { if (!key.startsWith("filter.")) continue; const [,field,rawOperator] = key.split("."); const kind = policy.fields[field]; const operator = rawOperator as FilterOperator; if (kind && operators[kind].includes(operator) && value.length <= 200) filters.push({ field, operator, value }); }
  return { page, pageSize, search, sort, direction, filters };
}"#;

const QUERY_CONTRACT_TEST: &str = r#"import { describe, expect, it } from "vitest";
import { parseListQuery } from "./query-contract";
const policy = { fields: { name: "string", age: "number" }, searchable: ["name"], sortable: ["name","age"], maxPageSize: 100 } as const;
describe("query allowlist", () => { it("clamps pagination and rejects unknown fields/operators", () => { const query = parseListQuery(new URLSearchParams("page=-1&pageSize=999&sort=secret&direction=desc&filter.name.contains=a&filter.age.contains=x"), policy); expect(query.page).toBe(1); expect(query.pageSize).toBe(100); expect(query.sort).toBe("name"); expect(query.filters).toEqual([{ field: "name", operator: "contains", value: "a" }]); }); });"#;
