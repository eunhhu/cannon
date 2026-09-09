# 조사한 외부 기준

확인일: 2026-09-09. 아래는 설계 시 참고한 1차 자료다. Cannon의 성능·제품 완성·보안 적합성 증거가 아니다. 구현 시 지원 버전과 클라이언트/플랫폼을 다시 확인하고 lock 및 검사 계약에 고정한다.

## S1 — Language Server Protocol

- [공식 안내](https://microsoft.github.io/language-server-protocol/)
- [3.18 specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/)

공식 안내는 3.18을 안내한다. LSP는 편집기와 언어 서버의 표준 기능 연결에 사용한다. Cannon의 정책 비교/실행 근거/변경 적용 전체 의미를 LSP가 정해주는 것은 아니다. CN-020/024는 실제 지원 capability와 클라이언트를 고정해 시험한다.

## S2 — Model Context Protocol

- [2025-11-25 specification](https://modelcontextprotocol.io/specification/2025-11-25)
- [Base protocol](https://modelcontextprotocol.io/specification/2025-11-25/basic)

고정된 공개 사양을 연결 계약의 참고로 사용한다. JSON-RPC, 기능 협상, tool/resource 연결, 사용자 동의와 자료 보호 요구를 검토한다. 프로토콜 자체가 Cannon 업무 권한이나 검토 완료를 증명하지 않는다. 미래 새 사양으로의 변경은 별도 호환성 작업이다.

## S3 — NIST Secure Software Development Framework

- [SP 800-218 / SSDF 1.1, Final](https://csrc.nist.gov/pubs/sp/800/218/final)
- [SP 800-218 Rev.1 / SSDF 1.2, Initial Public Draft](https://csrc.nist.gov/pubs/sp/800/218/r1/ipd)

확인한 1.2 페이지는 Initial Public Draft 상태다. 확정 표준처럼 쓰지 않는다. 안전한 개발 과정·개발 산출물 보호·취약점 대응·원인 개선을 빠뜨리지 않기 위한 참고 틀로 1.1을 사용하고, 차기 문서의 상태를 구현 단계에서 다시 확인한다. 법적 준수나 인증을 주장하지 않는다.

## S4 — SLSA 1.2

- [Specification](https://slsa.dev/spec/v1.2/)
- [Build requirements](https://slsa.dev/spec/v1.2/build-requirements)

공급망의 build provenance와 생산자/빌드 플랫폼 책임을 검토하는 틀이다. 문서나 SBOM 하나를 만들었다고 특정 level을 달성하지 않는다. 실제 빌드 환경과 출처 확인을 검증해야 한다.

## S5 — GitHub Actions secure use

- [공식 secure-use 지침](https://docs.github.com/en/actions/reference/security/secure-use)

최소 권한, 고정된 Action 참조, 비밀 처리, 빌드/배포 경계를 workflow 검토에 사용한다. 이 로드맵 커밋은 workflow나 저장소 보호 규칙을 수정하지 않는다.

## S6 — 접근성

- [W3C WCAG 2.2](https://www.w3.org/TR/WCAG22/)

VS Code의 사용자 정의 웹 뷰/검토 화면에 적용할 접근성 목표로 WCAG 2.2 AA를 제안한다. 키보드·포커스·스크린 리더·색상 외 정보 표현을 실제 검사한다. 아직 적합성 평가를 수행했다는 뜻은 아니다.

## S7 — OpenTelemetry

- [Traces](https://opentelemetry.io/docs/concepts/signals/traces/)

운영 동작을 연결하는 trace/span 개념의 참고다. 운영 telemetry와 Cannon의 의미적 실행 근거는 구별한다. telemetry가 존재한다고 policy correctness나 최신 소스와의 일치를 자동 증명하지 않는다.

## 저장소 자체 근거

현재 기능과 과거 exact-commit CI는 [baseline.md](baseline.md)에 고정했다. 외부 표준은 선택과 요구사항을 돕지만 이 저장소의 미구현 기능을 대신하지 못한다.
