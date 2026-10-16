#!/usr/bin/env python3
"""Valida artefactos documentales; no ejecuta un servidor MCP ni Cucumber.

Requiere Python 3.11+ y jsonschema 4.x (Draft 2020-12).
No utiliza red y no instala paquetes automáticamente.
"""
from __future__ import annotations
import csv
import json
import platform
import re
import sys
from datetime import datetime, timezone
from importlib.metadata import version
from pathlib import Path

ROOT = Path(__file__).resolve().parent
OUT = ROOT / 'referencia_ejecutable/resultados'
# En el repositorio los capítulos están en esta carpeta y las ADR en docs/adr/.
CHAPTERS = sorted(ROOT.glob('[0-9][0-9]_*.md'))
ADRS = [p for p in sorted((ROOT.parent/'adr').glob('[0-9][0-9][0-9][0-9]-*.md')) if not p.name.startswith('0000-')]

def load(path: str):
    return json.loads((ROOT/path).read_text(encoding='utf-8'))

def read_csv(path: str):
    with (ROOT/path).open(encoding='utf-8-sig',newline='') as handle:
        return list(csv.DictReader(handle))

def main() -> int:
    try:
        from jsonschema import Draft202012Validator
    except ImportError:
        print('ERROR: falta jsonschema. Instale una versión 4.x en su entorno de validación. No se han validado los schemas.',file=sys.stderr)
        return 2
    checks=[]
    def check(name: str, condition: bool, detail: str=''):
        checks.append({'check':name,'passed':bool(condition),'detail':detail})
    schemas={}
    for path in sorted((ROOT/'contratos/esquemas').glob('*.json')):
        key=path.relative_to(ROOT).as_posix()
        try:
            schemas[key]=json.loads(path.read_text(encoding='utf-8'))
            Draft202012Validator.check_schema(schemas[key])
            check('schema:'+key,True)
        except Exception as exc:
            check('schema:'+key,False,str(exc))
    cases=load('contratos/validacion_casos.json')
    for case in cases:
        errors=list(Draft202012Validator(schemas[case['schema']]).iter_errors(load(case['example'])))
        actual_valid=not errors
        check('payload:'+case['example'],actual_valid==case['valid'],
              'válido' if actual_valid else '; '.join(e.message for e in errors)[:1200])
    messages=0
    for path in sorted((ROOT/'contratos/ejemplos').glob('*.jsonl')):
        for no,line in enumerate(path.read_text(encoding='utf-8').splitlines(),1):
            if not line.strip(): continue
            x=json.loads(line); messages+=1
            check(f'jsonl:{path.name}:{no}',x.get('jsonrpc')=='2.0')
            if 'moderno_solicitudes' in path.name:
                m=x.get('params',{}).get('_meta',{})
                check(f'meta:{no}',m.get('io.modelcontextprotocol/protocolVersion')=='2026-07-28' and 'io.modelcontextprotocol/clientCapabilities' in m)
                check(f'sin_initialize:{no}',x.get('method') not in ('initialize','notifications/initialized'))
            if 'moderno_respuestas' in path.name and 'result' in x:
                r=x['result']
                check(f'result_type:{no}',r.get('resultType')=='complete')
                if 'tools' in r or 'supportedVersions' in r:
                    check(f'cache_metadata:{no}',r.get('ttlMs',-1)>=0 and r.get('cacheScope') in ('public','private'))
                diagnostic=r.get('_meta',{}).get('io.ommatix/action_timings',{}).get('freshness_diff')
                if diagnostic is not None:
                    errors=list(Draft202012Validator(schemas['contratos/esquemas/action_freshness_diff.meta.schema.json']).iter_errors(diagnostic))
                    check(f'frescura_d0_meta:{no}',not errors)
                    check(f'frescura_d0_rechazo:{no}',r.get('structuredContent',{}).get('error',{}).get('code')=='STALE_SNAPSHOT')
                    payload=json.dumps(r.get('structuredContent',{}))+json.dumps(r.get('content',[]))
                    check(f'frescura_d0_solo_meta:{no}',all(key not in payload for key in ('freshness_diff','total_tiles','group_tiles','touches_target')))
                if 'structuredContent' in r:
                    check(f'contenido_consistente:{no}',json.loads(r['content'][0]['text'])==r['structuredContent'])
    # Estos dos archivos forman un diálogo ilustrativo completo por IDs.
    reqs=[json.loads(line) for line in (ROOT/'contratos/ejemplos/mcp_moderno_solicitudes.jsonl').read_text(encoding='utf-8').splitlines() if line.strip()]
    resps=[json.loads(line) for line in (ROOT/'contratos/ejemplos/mcp_moderno_respuestas.jsonl').read_text(encoding='utf-8').splitlines() if line.strip()]
    check('dialogo_moderno_ids_emparejados',{x['id'] for x in reqs}=={x['id'] for x in resps} and len({x['id'] for x in reqs})==len(reqs) and len({x['id'] for x in resps})==len(resps))
    for req in reqs:
        if req.get('method')=='tools/call':
            name=req['params']['name']
            errors=list(Draft202012Validator(schemas[f'contratos/esquemas/{name}.input.schema.json']).iter_errors(req['params']['arguments']))
            check(f'argumentos_ejemplo_mcp:{req["id"]}',not errors)
    requirements=read_csv('matrices/requisitos_trazabilidad.csv')
    planned=read_csv('matrices/plan_pruebas.csv')
    scenarios=read_csv('matrices/gherkin_trazabilidad.csv')
    rid={x['id'] for x in requirements}; tid={x['id'] for x in planned}
    check('csv_columnas_completas',all(None not in row and all(v is not None for v in row.values()) for row in requirements+planned+scenarios))
    check('requisitos_unicos',len(rid)==len(requirements))
    check('pruebas_planificadas_unicas',len(tid)==len(planned))
    source_refs={x['id'] for x in load('fuentes/referencias.json')}
    check('capitulos_encontrados',len(CHAPTERS)==13,str(len(CHAPTERS)))
    check('adrs_encontradas',len(ADRS)>=12,str(len(ADRS)))
    for doc in CHAPTERS+ADRS:
        text=doc.read_text(encoding='utf-8')
        refs=set(re.findall(r'\bS\d{2}\b',text))
        check('referencias:'+doc.name,refs<=source_refs,str(sorted(refs-source_refs)))
    found={}
    feature_files=sorted((ROOT/'features').glob('*.feature'))
    for p in feature_files:
        text=p.read_text(encoding='utf-8')
        check('gherkin_cabecera:'+p.name,text.startswith('# language: es\n') and 'Característica:' in text)
        blocks=re.split(r'(?m)^\s*Escenario:\s*',text)[1:]
        for block in blocks:
            idmatch=re.match(r'(BDD-\d+)\b',block)
            if not idmatch:
                check('gherkin_id:'+p.name,False);continue
            bid=idmatch.group(1)
            check('gherkin_id_unico:'+bid,bid not in found)
            found[bid]=p.relative_to(ROOT).as_posix()
            check('gherkin_estructura:'+bid,all(re.search(r'^\s*'+step+r'\b',block,re.M) for step in ('Dado','Cuando','Entonces')))
    check('escenarios_matriz_completos',set(found)=={x['scenario'] for x in scenarios})
    for row in scenarios:
        check('trazabilidad:'+row['scenario'],row['requirement'] in rid and row['planned_test'] in tid and found.get(row['scenario'])==row['file'])
    # DOC-01 (RNF-10): cada requisito está ligado a pruebas que existen, a evidencia que
    # existe, a ADR que existen y a un propietario; ninguna prueba queda sin requisito,
    # escenario o riesgo; los estados salen del vocabulario de las matrices. No decide si la
    # evidencia basta (eso es el estado de cada prueba), sólo que el vínculo no está roto.
    risks=read_csv('matrices/riesgos.csv')
    tests_by_prefix={}
    for x in planned:
        tests_by_prefix.setdefault(x['id'].split('-')[0],[]).append(x['id'])
    def expand(text: str):
        """Identificadores de prueba que nombra `text` (rangos `UT-01..UT-08`, `E2E-01..E2E-03`, prefijos `UT/SEC`) y lo que no resuelve."""
        collected,unresolved=set(),[]
        for token in re.split(r'[;,/]|\s+y\s+|\s+and\s+',text):
            token=token.strip().rstrip('.')
            if not token:
                continue
            span=re.fullmatch(r'([A-Z0-9]+)-(\d+)\.\.(?:\1-)?(\d+)',token)
            if span:
                width=len(span.group(2))
                ids=[f'{span.group(1)}-{n:0{width}d}' for n in range(int(span.group(2)),int(span.group(3))+1)]
                (collected.update(ids) if set(ids)<=tid else unresolved.append(token))
            elif token in tid:
                collected.add(token)
            elif token in tests_by_prefix:
                collected.update(tests_by_prefix[token])
            else:
                unresolved.append(token)
        return collected,unresolved
    referenced=set()
    adr_numbers={p.name[:4] for p in ADRS}
    repo=ROOT.parent.parent
    for x in requirements:
        named,unresolved=expand(x['pruebas_planificadas'])
        referenced|=named
        check('doc01_pruebas_existen:'+x['id'],bool(named) and not unresolved,str(unresolved))
        check('doc01_adr_existen:'+x['id'],
              bool(re.findall(r'ADR-\d{4}',x['adr'])) and {n[4:] for n in re.findall(r'ADR-\d{4}',x['adr'])}<=adr_numbers,x['adr'])
        check('doc01_propietario:'+x['id'],bool(x['propietario'].strip()))
        paths=re.findall(r'(?<![\w/.])((?:docs|src|tools)/[\w./-]+\.(?:md|rs|py|json|csv|toml))',x['evidencia'])
        missing=[p for p in paths if not (repo/p).exists()]
        check('doc01_evidencia_existe:'+x['id'],bool(x['evidencia'].strip()) and not missing,str(missing))
    for x in scenarios:
        referenced.add(x['planned_test'])
    for x in risks:
        named,unresolved=expand(x['validation'])
        referenced|=named
        check('doc01_riesgo_pruebas:'+x['id'],bool(named) and not unresolved,str(unresolved))
    for adr in ADRS:
        section=re.search(r'(?ms)^## Required validation\s*$(.*?)(?=^## |\Z)',adr.read_text(encoding='utf-8'))
        mentioned=set(re.findall(r'\b(?:UT|IT|CT|SEC|E2E|S4R|PERF|EV|REL|DOC)-\d+(?:\.\.(?:UT|IT|CT|SEC|E2E|S4R|PERF|EV|REL|DOC)-\d+)?',section.group(1))) if section else set()
        unresolved=[t for t in mentioned if expand(t)[1]]
        check('doc01_adr_pruebas:'+adr.name,not unresolved,str(unresolved))
    for x in planned:
        check('doc01_prueba_enlazada:'+x['id'],x['id'] in referenced)
    plan_statuses={'IMPLEMENTADO_EN_RUST_S1','IMPLEMENTADO_EN_RUST_S2','IMPLEMENTADO_EN_RUST_S4','EJECUTADO_SUPERVISADO',
                   'EJECUTADO_SUPERVISADO_PARCIAL','EJECUTADO_PARCIAL_SPIKE_S0','PENDIENTE_PRODUCTO','IMPLEMENTADO_EN_RUST_EXPERIMENTAL'}
    scenario_statuses={'EJECUTADO_SOBRE_NUCLEO_S1','EJECUTADO_SOBRE_NUCLEO_S2','EJECUTADO_SOBRE_CONTRATO_S2','EJECUTADO_SOBRE_CONTRATO_S3_DOBLE','EJECUTADO_SOBRE_NUCLEO_EXPERIMENTAL'}
    check('doc01_estados_de_pruebas',all(x['estado_aceptacion_producto'] in plan_statuses for x in planned),
          str(sorted({x['estado_aceptacion_producto'] for x in planned}-plan_statuses)))
    check('doc01_estados_de_escenarios',all(x['status'] in scenario_statuses for x in scenarios),
          str(sorted({x['status'] for x in scenarios}-scenario_statuses)))
    # ADR-0029: un requisito aceptado por cambio de alcance cita la nota de alcance (§6.7 o una entrada de CHANGELOG que existe).
    requirement_statuses={'PENDIENTE_IMPLEMENTACION_O_VALIDACION','ACEPTADO_POR_ALCANCE'}
    check('doc01_estado_de_requisitos',all(x['estado_producto'] in requirement_statuses for x in requirements),
          str(sorted({x['estado_producto'] for x in requirements}-requirement_statuses)))
    scope_block=(ROOT/'06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md').read_text(encoding='utf-8')
    changelog=(ROOT/'CHANGELOG.md').read_text(encoding='utf-8')
    for x in requirements:
        if x['estado_producto']=='ACEPTADO_POR_ALCANCE':
            cites_block='docs/thesis/06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md' in x['evidencia'] and '§6.7' in x['evidencia'] and '\n## 6.7 ' in scope_block
            cites_entry=any(f'\n## {v} ' in changelog for v in re.findall(r'CHANGELOG (1\.0\.\d+)',x['evidencia']))
            check('doc01_nota_de_alcance:'+x['id'],cites_block or cites_entry,x['evidencia'])
    oracle=load('referencia_ejecutable/resultados/oraculo_report.json')
    check('reporte_oraculo_aprobado',oracle.get('success') is True and oracle.get('tests_run')==48,
          'Se lee el informe previo; para volver a ejecutar use referencia_ejecutable/run_tests.py')
    report={
        'timestamp_utc':datetime.now(timezone.utc).isoformat(),'python':platform.python_version(),
        'jsonschema_version':version('jsonschema'),
        'scope':'Schemas de dominio, ejemplos y consistencia estructural de documentación. NO conformidad MCP ni ejecución Cucumber.',
        'counts':{'schemas':len(schemas),'payload_cases':len(cases),'valid_examples':sum(x['valid'] for x in cases),
                  'invalid_examples':sum(not x['valid'] for x in cases),'jsonl_messages':messages,
                  'feature_files':len(feature_files),'gherkin_scenarios':len(found),
                  'requirements':len(rid),'planned_tests':len(tid),'references':len(source_refs),
                  'adrs':len(ADRS)},
        'checks_run':len(checks),'checks_failed':sum(not x['passed'] for x in checks),
        'success':all(x['passed'] for x in checks),'checks':checks,
        'not_run':['parser oficial Gherkin','step definitions BDD','conformidad MCP con un servidor','compilación Rust','benchmarks del producto']}
    OUT.mkdir(parents=True,exist_ok=True)
    (OUT/'validacion_documental.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    lines=[('PASS ' if x['passed'] else 'FAIL ')+x['check']+' '+x['detail'] for x in checks]
    (OUT/'validacion_documental.log').write_text('\n'.join(lines)+'\n',encoding='utf-8')
    print(json.dumps({k:report[k] for k in ('counts','checks_run','checks_failed','success')},ensure_ascii=False,indent=2))
    return 0 if report['success'] else 1

if __name__=='__main__':
    raise SystemExit(main())
