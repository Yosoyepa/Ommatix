"""Ejecute con Python 3.10+; sólo usa la biblioteca estándar."""
from pathlib import Path
import datetime,io,json,platform,sys,unittest
ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT))
stream=io.StringIO()
suite=unittest.defaultTestLoader.discover(str(ROOT/'tests'))
result=unittest.TextTestRunner(stream=stream,verbosity=2).run(suite)
(ROOT/'resultados').mkdir(exist_ok=True)
(ROOT/'resultados/oraculo_unittest.log').write_text(stream.getvalue(),encoding='utf-8')
report={
 'timestamp_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
 'python':sys.version,'platform':platform.platform(),
 'tests_run':result.testsRun,'failures':len(result.failures),'errors':len(result.errors),'skipped':len(result.skipped),
 'success':result.wasSuccessful(),
 'scope':'Oráculos escalares y estado simulado. No acredita Rust, MCP, OCR, GPU ni control del escritorio.',
 'not_run':['compilacion Rust','conformidad MCP','BDD con Cucumber','inferencia de modelos','acciones Windows','benchmark de producto']}
(ROOT/'resultados/oraculo_report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(stream.getvalue());print(json.dumps(report,ensure_ascii=False,indent=2))
sys.exit(0 if result.wasSuccessful() else 1)
