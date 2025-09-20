# CI/CD - GitLab Pipeline

Este projeto está configurado com GitLab CI/CD para construir e publicar automaticamente imagens Docker.

## Configuração

### 1. Variáveis de Ambiente no GitLab

Configure as seguintes variáveis no seu projeto GitLab (Settings > CI/CD > Variables):

- `CI_REGISTRY_USER`: Usuário do registry GitLab
- `CI_REGISTRY_PASSWORD`: Senha/token do registry GitLab

### 2. Registry GitLab

O pipeline está configurado para usar o registry padrão do GitLab:
- Registry: `registry.gitlab.com/seu-usuario/owl-face-rec`
- Imagens geradas: 
  - `registry.gitlab.com/seu-usuario/owl-face-rec:branch-name`
  - `registry.gitlab.com/seu-usuario/owl-face-rec:latest` (apenas para main/master)

## Como Funciona

### Build Automático
- **Trigger**: Qualquer push para branches ou merge requests
- **Cache**: Utiliza cache do Rust para acelerar builds futuros
- **Multi-stage**: Build otimizado com imagem final menor

### Tags de Imagem
- **Branch**: `owlfacerec:branch-name`
- **Latest**: `owlfacerec:latest` (apenas para main/master)
- **Tags**: `owlfacerec:tag-name` (para releases)

## Usando a Imagem em Outros Projetos

### 1. Docker Compose

```yaml
version: '3.8'
services:
  owlfacerec:
    image: registry.gitlab.com/seu-usuario/owl-face-rec:latest
    ports:
      - "3000:3000"
    environment:
      POSTGRES_HOST: postgres
      POSTGRES_DB: owlfacerec
      # ... outras variáveis
```

### 2. Docker Run

```bash
docker run -d \
  --name owlfacerec \
  -p 3000:3000 \
  -e POSTGRES_HOST=seu-postgres-host \
  -e POSTGRES_DB=owlfacerec \
  registry.gitlab.com/seu-usuario/owl-face-rec:latest
```

### 3. Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: owlfacerec
spec:
  replicas: 1
  selector:
    matchLabels:
      app: owlfacerec
  template:
    metadata:
      labels:
        app: owlfacerec
    spec:
      containers:
      - name: owlfacerec
        image: registry.gitlab.com/seu-usuario/owl-face-rec:latest
        ports:
        - containerPort: 3000
        env:
        - name: POSTGRES_HOST
          value: "postgres-service"
```

## Limpeza Automática

O pipeline inclui um job de limpeza que remove imagens antigas, mantendo apenas as últimas 5 versões para economizar espaço no registry.

## Monitoramento

- Acesse **CI/CD > Pipelines** no GitLab para ver o status dos builds
- Logs detalhados estão disponíveis em cada job
- Notificações podem ser configuradas nas configurações do projeto

## Troubleshooting

### Build Falha
1. Verifique se todas as dependências estão no `Cargo.toml`
2. Confirme se o `Dockerfile` está correto
3. Verifique os logs do pipeline para erros específicos

### Push Falha
1. Verifique se as variáveis `CI_REGISTRY_USER` e `CI_REGISTRY_PASSWORD` estão configuradas
2. Confirme se o projeto tem permissões para push no registry
3. Verifique se o registry está acessível

### Imagem Não Encontrada
1. Confirme se o build foi executado com sucesso
2. Verifique se a tag da imagem está correta
3. Confirme se o registry está acessível do ambiente de destino
