# Production 품질·검증·출시 전략

상태: 목표와 검증 계획. 아래 수치는 측정 결과나 고객 SLA가 아니다. 현재 CI는 [baseline](baseline.md)만 증명한다. 기능이 있거나 테스트가 많다는 이유로 출시 게이트를 통과시키지 않는다.

## 1. 시험을 작업 시작부터 설계

각 task는 독립적으로 정한 입력/기대값과 실패 사례를 먼저 정의한다. 구현 agent와 검증 agent는 같은 저장소 계약을 읽되, 새 구현의 출력으로 정답 파일을 만들지 않는다. 기존 사례의 수정/삭제는 별도의 acceptance change로 기록한다.

| 층 | 필수 검사 | 잡아야 할 실패 |
|---|---|---|
| 언어 | 독립 conformance, parse/type/runtime 오류, property/metamorphic | ID 변경, 정수 경계, Unicode, 평가 순서, 숨겨진 coercion |
| 엔진 이식 | TS/Rust 차등 실행 및 독립 oracle | 같은 버그를 공유하는 양쪽 구현, 잘못된 결과 정규화 |
| 프로젝트 | full rebuild와 incremental 대조, dependency 변화 | stale cache, 새로운 연결 누락, 실패한 원래 사례 삭제 |
| 편집/적용 | 문서 버전, 중간 종료, 충돌, 복구 | 오래된 결과 표시, 검토하지 않은 후보 적용, 조용한 소스 손실 |
| 프로토콜 | 계약 fixture, unknown version, 취소/재연결, 큰 응답 | 버전 불일치, 이전 요청의 늦은 응답, 클라이언트별 의미 차이 |
| UI | 실제 Extension Host, keyboard/screen reader, 흐름 E2E | 정답/근거 혼동, 클릭은 되지만 실제 엔진 미연결 |
| AI | 결정적 fake provider + 허용된 실제 provider smoke | 정답 동시 변경, 문맥 누락, 출처 없는 설명을 사실로 취급 |
| 데이터/운영 | 동시성, 중복 요청, 장애, migration/recovery | 손실, 중복 반영, rollback이 불가능한데 가능하다고 표시 |
| 배포 | 깨끗한 머신 설치, 버전 교체/복원, artifact 검증 | 개발 도구가 있어야만 실행되는 배포본, 잘못된 타깃 |

무작위 검사에는 seed와 최소화한 실패 사례를 남긴다. 시간 제한으로 검사가 중단되면 pass가 아니다. 알려진 재현 입력을 회귀 테스트로 보존한다. 안전성 검증은 소유/승인된 테스트 환경에서 수행한다.

## 2. 반드시 자동화할 제품 시나리오

A. 오프라인 새 프로젝트에서 데이터 모델·규칙·시나리오를 작성하고 trace를 본다.
B. 핵심 경계 조건을 바꾸고 candidate의 정답도 바꾼다. candidate는 통과해도 원래 실패를 드러낸다.
C. 원래 테스트를 삭제하고 입력값까지 바꾼다. historical replay는 원래 사례를 계속 사용한다.
D. 선언/필드의 이름과 파일 위치만 바꾼다. identity와 원래 데이터의 의미가 유지된다.
E. 정책/소유권/외부 효과/데이터 이행을 바꾼다. 각 변경과 확인하지 못한 경계가 분리된다.
F. 제안 검토 중 다른 편집이 발생한다. 예전 검토는 적용되지 않고 새 문서 결과와 혼동되지 않는다.
G. 파일 적용·업데이트·데이터 이행 중 프로세스를 종료한다. 재시작 후 손실 없이 복구하거나 명확한 충돌로 멈춘다.
H. AI가 중단되거나 사용할 수 없다. 사람이 같은 기능을 직접 수행하고 대화 없이 이어간다.
I. 배포한 버전과 로컬 수정 버전이 다르다. 운영 관측이 잘못된 소스에 연결되지 않는다.
J. 두 번째 도메인인 데이터 변환에서도 A~I 중 적용 가능한 흐름을 재현한다.

매 시나리오에 source revision, 환경/engine version, 입력 종류(real/fake/replay), 결과와 화면/trace 근거를 연결한다. UI 자동화 성공과 인간 이해 검증은 다른 결과로 남긴다.

## 3. 제안 성능 예산

CN-001/004에서 기준 머신과 실제 corpus를 정한 뒤 아래 초안을 확정하거나 이유와 함께 바꾼다. 대상 밖 입력은 지원 한도로 명시한다. 현재 prototype이 이 크기를 지원한다는 뜻은 아니다.

| 대상 | beta 목표 초안 | 측정 방법 |
|---|---|---|
| 작은 수정 후 진단 | 1,000 선언 corpus에서 p95 150ms 이하 | warm 1,000회, 최초 실행과 분리, coordinator→진단 수신 |
| 프로젝트 열기 | 10,000 선언 corpus에서 p95 2초 이하 | cold 30회, cache 삭제 조건/파일 수 명시 |
| 기본 검토 | 대표 100사례에서 p95 2초 이하 | baseline/candidate/history 전체, trace 설정 고정 |
| 취소 반응 | 엔진 checkpoint 기반 작업 p95 100ms 이하 | host callback/실제 외부 작업은 별도 측정 |
| 메모리 | 위 큰 corpus의 엔진 RSS peak 1GiB 이하 | 최소 4 logical cores/16GiB 기준, OS/빌드 설정 기록 |
| 장시간 세션 | 8시간 편집 재생에서 한도 초과나 지속적인 미해제 증가 없음 | snapshot history/trace cap 고정, 종료 후 baseline 비교 |

이 숫자는 외부 연구 결과가 아니라 Cannon 초기 설계 예산이다. p50/p95/p99, 표본 수, commit, hardware/OS, 입력 크기, cache와 tracing 상태를 함께 저장한다. median만 비교하거나 좋은 입력만 고르지 않는다. CI 머신 노이즈가 크면 전용 runner에서 반복 확인하며 기준을 완화하는 커밋과 최적화 커밋을 분리한다.

생산성은 작성량이 아니라 **같은 품질에서 요구 변경→검토→검증 완료까지의 사람 시간**으로 측정한다. 기존 환경/새 환경 × AI 없음/있음의 네 조건을 교차 배정하고 학습·반복 효과를 기록한다. beta 초안 목표는 대표 변경의 중앙값 25% 감소 및 새로 놓친 핵심 정책 변경 증가 없음이다. 작은 표본으로 일반적인 우위를 주장하지 않고 효과 크기·불확실성을 남긴다.

## 4. 릴리스 게이트

| 게이트 | 통과 조건 | 증거 소유자 |
|---|---|---|
| G0 기준선 | 새 환경의 기존 검사 성공, 갭/위험/지원 초안·task DAG 확보 | coordinator + verifier |
| G1 의미 동등성 | 기존 언어 독립 적합성·오류/trace/위치·원래 기대값 보존 | language + verifier |
| G2 변경 무결성 | project snapshot/검토/적용/복구의 중단·충돌 시험 통과 | workspace + verifier |
| G3 local alpha | 실제 파일 기반 IDE 흐름, 오프라인, 접근성, 두 도메인 | tooling + verifier |
| G4 team beta | 선택적 AI와 하나의 실제 저장 어댑터, 명시적 전송·권한·실패 처리 | integration + operator |
| G5 운영 준비 | 설치/업데이트/복구, 보안 검토, 성능 예산, 문서/지원 준비 | release + independent review |
| G6 파일럿 | 초안 3개 대표 프로젝트, 4주 사용; 차단 결함·복구·지원 부담 검토 | product owner + pilot owners |
| G7 GA 승인 | G0~G6 완료, 최종 인간 QA, 책임자·지원·배포 범위 확정 | release owner |

파일럿 기간/수는 관측 계획이지 완료 예상 기간이 아니다. 고객이 없으면 현실적인 내부 작업으로 대체하되 customer validation으로 표기하지 않는다. 독립 보안 검토자가 없으면 구현자 self-review를 independent review로 기록하지 않는다.

차단 결함에는 데이터 손실, 검토와 다른 변경 적용, 권한 밖 동작, 비밀 노출, stale 결과의 현재 근거 승격, 핵심 의미 회귀가 포함된다. 열린 차단 결함이 있으면 출시하지 않는다. 덜 심한 결함도 지원 범위와 알려진 제한에 기록하고 owner가 허용 여부를 결정한다.

## 5. 릴리스와 업데이트

Dev→alpha→beta→release candidate→stable을 분리한다. release candidate는 정확한 commit과 artifact digest, toolchain/lock, 플랫폼 매트릭스, 설치/복원 검사, SBOM/provenance, 변경/이행 안내를 갖는다. 공급망 provenance는 빌드 출처 정보이며 프로그램의 정답 증명이 아니다.[S4]

서명/배포 권한은 승인된 릴리스 역할이 관리한다. 같은 태그를 다른 바이너리로 바꾸지 않는다. OS/CPU별 실제 실행을 하지 않았다면 해당 타깃 지원 완료로 표시하지 않는다. TypeScript 기본 엔진 퇴역은 release notes와 지원 기간, 비교 검사 보존 계획이 있어야 한다.

rollback은 앱 버전, project format, business data, 이미 발생한 외부 효과를 각각 확인한다. 역변환할 수 없는 이행은 백업/forward-fix/수동 복구 절차를 release 전에 검증한다. 손상된 metadata나 journal을 삭제해서 성공처럼 재시작하지 않는다.

## 6. 최종 QA 패키지와 증거 형식

패키지: 설치/제거/업데이트 지침, 두 도메인 소스와 기대 결과, A~J 실행 절차, 외부 전송/권한 설명, baseline와 candidate 비교, 장애/복구 기록, 지원 OS/버전, 알려진 제한, 운영·보안 연락 경로, 정확한 원격 CI와 artifact.

각 task evidence는 `commit`, `test_command`, `environment`, `input_or_fixture`, `expected`, `observed`, `exit_code`, `artifact_or_run`, `limitations`를 남긴다. 원격 CI pending/실패/취소와 실제 tool 부재를 구분한다. 완료한 다른 커밋의 초록색을 재사용하지 않는다.

표준 참고 [S4](sources.md), UI 접근성 목표 참고 [S6](sources.md). 이를 언급한 것만으로 인증·적합성을 주장하지 않는다.
