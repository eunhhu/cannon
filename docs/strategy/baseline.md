# 조사 기준과 gap 분석

기준 커밋: `7b856994e92149f223e37a08a714c0fec6f728ca` · 확인일: 2026-09-09.

이 파일은 그 시점의 기록이다. 이후 구현 완료 여부는 최신 main, STATUS, 코드와 정확한 CI run으로 갱신한다. 과거 채팅·ZIP·VALIDATION.json을 새 커밋의 검증으로 사용하지 않는다.

## 실제 존재하는 것

| 영역 | 현재 구현 | 다음 격차 |
|---|---|---|
| Rust scalar engine | 타입 입력, checked safe integer, UTF-16 문자열/위치, 조건/단락 평가, 실제 trace | 레코드·규칙·전이의 전체 파일 문법과 호환성 |
| 관찰/취소 | 동일 평가기의 observer, 보관 옵션, 문자열/단계/이벤트 예산, 협력적 취소 | 분석 취소·큐 backpressure·요청 순서·프로세스 강제 격리 |
| 세션 | 불변 단일 정책/사례 snapshot, 입력/기대값 분리, 정확한 메모리 적용, 오래된 결과 배제 | 다중 파일·다중 사례·영속 근거·저장 중 충돌/복구 |
| 프로젝트 | Rust host API의 모듈/소유자/import/export/타입 연결 DAG | 런타임 프로젝트 형식·패키지 로딩·증분 분석 |
| 프로젝트 실행 | 모든 정책을 deterministic eager 순서로 한 번씩 실행 | 언어 함수 호출과 effectful workflow의 별도 의미 설계 |
| 검토 | 구조/입력/정답 변경 구분, 양쪽 그래프 영향 후보, 이전 다중 출력 사례 재검사 | 영속 review ID, 권한·데이터 이행·배포 영향까지 포함 |
| TypeScript reference | 단일 .intent 파일의 레코드·규칙·순수 전이·시나리오·비교·CLI | Rust 적합성 완료 뒤 기본 엔진 전환·reference 퇴역 |
| 제품 | Rust CLI/예제, npm reference CLI, 테스트/문서/CI | 실제 IDE 배포본·AI 클라이언트·운영 어댑터·릴리스/지원 |

현재 `crates/cannon-core`는 외부 Rust 패키지 없이 동작한다. 이것은 미래에도 직접 암호·JSON·DB 구현을 작성해야 한다는 원칙이 아니다. 의존성 추가는 검토·버전 고정·라이선스·공급망 검증으로 관리한다.

## 확인한 근거

- [기준 커밋](https://github.com/eunhhu/cannon/commit/7b856994e92149f223e37a08a714c0fec6f728ca)
- [STATUS](https://github.com/eunhhu/cannon/blob/7b856994e92149f223e37a08a714c0fec6f728ca/STATUS.md)
- [아키텍처 계약](https://github.com/eunhhu/cannon/blob/7b856994e92149f223e37a08a714c0fec6f728ca/spec/architecture.md)
- [언어 의미](https://github.com/eunhhu/cannon/blob/7b856994e92149f223e37a08a714c0fec6f728ca/spec/semantics.md)
- [native CI run 34309264756](https://github.com/eunhhu/cannon/actions/runs/34309264756): 같은 커밋의 Linux/Windows/macOS debug/release, Clippy, 기존 TS 검사, 적합성 대조와 예제 성공. 이 계획을 작성하며 job 상태를 다시 조회했다.
- [reference CI run 34309264701](https://github.com/eunhhu/cannon/actions/runs/34309264701): 기존 커밋 검증 이력. 다음 구현 작업은 자기 커밋에서 다시 검사해야 한다.

STATUS의 테스트 집계는 Rust 194개(기존 132+프로젝트/경계 62), reference 90개, 독립 표현식 60개/타입 입력 30개다. 테스트 개수는 커버리지·제품 완성·보안 보증이 아니다. 이번 계획 작성은 Rust 코드 재실행을 의미하지 않는다.

## 유지할 계약

`AGENTS.md`, `spec/semantics.md`, `docs/native-expression-slice.md`, `docs/native-observation.md`, `docs/native-bound-policies.md`, `docs/native-sessions.md`, `docs/native-projects.md`를 해당 변경 전에 읽는다. 특히 이름≠안정 ID≠snapshot≠실행근거, staged≠committed≠deployed를 유지한다.

## 미검증/전송 제한 산출물

대화에 있는 전체 Rust workbench와 편집기 후보는 원격 검증된 출발점이 아니다. `spec/delivery.md`와 STATUS에 기록된 전송 제한을 해결했다고 가정하지 않는다. 차단된 내용을 다른 경로·인코딩·권한으로 재전송하지 않는다. 정책을 준수하는 해결 절차와 독립 검증을 기록하기 전에는 이 후보를 신뢰 경로에 넣지 않는다. 이번 커밋은 전략 문서만 추가하며 그 코드의 재전송이 아니다.

## 가장 큰 제품 위험

1. 기능 조각이 API에만 존재하고 사용자 흐름이 연결되지 않는 위험: M2/M3의 파일→편집→검토→적용 E2E로 해소.
2. TS/Rust가 각자 다른 언어가 되는 위험: CN-002/004/011과 언어 버전 계약으로 차단.
3. 계획과 실제 상태가 또 다른 블랙박스가 되는 위험: 작업표·근거·출구 조건을 저장소에서 검사.
4. 에이전트가 구현과 정답을 동시에 바꿔 회귀를 지우는 위험: 독립 baseline과 검토 권한 분리.
5. 로컬 메모리의 안전성을 파일·DB·네트워크·멀티테넌트 보장으로 확장하는 위험: 경계별 별도 실패 시험.
