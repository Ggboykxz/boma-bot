{{/*
Chart name helper — used everywhere as the application selector/name.
*/}}
{{- define "boma-bot.name" -}}
{{- .Chart.Name | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Full qualified name: chart name alone for this chart; releases can override via "nameOverride"
if truly needed. Keeps boma-bot / boma-bot-app / boma-bot-worker recognizable.
*/}}
{{- define "boma-bot.fullname" -}}
{{- .Chart.Name | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels for every object this chart renders.
*/}}
{{- define "boma-bot.labels" -}}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version | quote }}
app.kubernetes.io/name: {{ include "boma-bot.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

